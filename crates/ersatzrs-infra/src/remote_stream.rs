use std::collections::VecDeque;
use std::io;
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
const REMOTE_STREAM_SEEK_PLACEHOLDER: &str = "{seek}";
const STDERR_TAIL_LIMIT: usize = 16 * 1024;
const STDERR_EXCERPT_LIMIT: usize = 2 * 1024;

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

impl RemoteStreamCommand {
    fn accepts_seek(&self) -> bool {
        self.args
            .iter()
            .any(|argument| argument == REMOTE_STREAM_SEEK_PLACEHOLDER)
    }

    fn with_seek(&self, seek_position: &str) -> Self {
        let mut command = self.clone();
        for argument in &mut command.args {
            if argument == REMOTE_STREAM_SEEK_PLACEHOLDER {
                seek_position.clone_into(argument);
            }
        }
        command
    }
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

/// Playback surface that owns a Remote Stream producer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteStreamSurface {
    /// HLS segmenter playback.
    Hls,
    /// Direct MPEG-TS playback.
    MpegTs,
    /// Scanner or metadata probe without a playback channel.
    Probe,
}

impl RemoteStreamSurface {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Hls => "hls",
            Self::MpegTs => "mpeg-ts",
            Self::Probe => "probe",
        }
    }
}

/// Safe identifiers attached to Remote Stream producer lifecycle logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteStreamSpawnContext {
    /// Playback or probe surface that started the producer.
    pub surface: RemoteStreamSurface,
    /// Stable channel number when playback owns the producer.
    pub channel_number: Option<String>,
}

impl RemoteStreamSpawnContext {
    /// Context for one HLS channel.
    #[must_use]
    pub fn hls(channel_number: impl Into<String>) -> Self {
        Self {
            surface: RemoteStreamSurface::Hls,
            channel_number: Some(channel_number.into()),
        }
    }

    /// Context for one direct MPEG-TS channel.
    #[must_use]
    pub fn mpeg_ts(channel_number: impl Into<String>) -> Self {
        Self {
            surface: RemoteStreamSurface::MpegTs,
            channel_number: Some(channel_number.into()),
        }
    }

    /// Context for a scanner or metadata probe.
    #[must_use]
    pub const fn probe() -> Self {
        Self {
            surface: RemoteStreamSurface::Probe,
            channel_number: None,
        }
    }
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
    /// ADR 0003 filter-graph transport file, when this spawn needed one.
    ///
    /// Held purely so it outlives the child: dropping this struct — on a clean
    /// exit, a kill, a cancelled request, or supervision teardown — deletes the
    /// file. `kill_on_drop(true)` on the child means there is no path that
    /// leaves `FFmpeg` running without this guard.
    filter_graph_file: Option<crate::filter_graph_file::FilterGraphFile>,
}

impl RemoteStreamProcess {
    /// Whether this spawn moved its filter graph to a transport file
    /// (ADR 0003).
    ///
    /// Exposed so tests and troubleshooting can confirm the transport choice
    /// without parsing logs; the file itself is owned here purely so it is
    /// deleted when this process handle drops.
    #[must_use]
    pub const fn uses_filter_graph_file(&self) -> bool {
        self.filter_graph_file.is_some()
    }

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
    let source_redacted = match &stream.source {
        RemoteStreamSource::Url(url) => value.replace(url, &redacted_source_label(&stream.source)),
        RemoteStreamSource::Script(_) => value.to_owned(),
    };
    sanitize_remote_stream_diagnostic(&source_redacted)
}

/// Sanitize and bound an operator-facing Remote Stream diagnostic excerpt.
///
/// Producer tools can echo signed media URLs, authentication headers, cookie
/// paths, or local filesystem locations to stderr. This helper keeps the error
/// class useful while ensuring those values never reach a log field.
#[must_use]
pub fn sanitize_remote_stream_diagnostic(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let urls_redacted = redact_diagnostic_urls(&normalized);
    let mut sanitized = Vec::new();
    let mut redact_following = 0_u8;

    for word in urls_redacted.split_whitespace() {
        if redact_following > 0 {
            sanitized.push("[redacted]".to_owned());
            redact_following = redact_following.saturating_sub(1);
            continue;
        }

        let key = diagnostic_assignment_key(word);
        if key.is_some_and(is_sensitive_diagnostic_key) {
            let has_inline_value = word.contains('=')
                || word
                    .split_once(':')
                    .is_some_and(|(_, value)| !value.is_empty());
            sanitized.push(format!(
                "{}=[redacted]",
                key.unwrap_or("secret").to_ascii_lowercase()
            ));
            if !has_inline_value {
                redact_following =
                    if key.is_some_and(|value| value.eq_ignore_ascii_case("authorization")) {
                        2
                    } else {
                        1
                    };
            }
        } else if looks_like_private_path(word) {
            sanitized.push("[redacted path]".to_owned());
        } else if let Some((key, value)) = diagnostic_assignment(word) {
            if looks_like_private_path(value) {
                sanitized.push(format!("{key}=[redacted path]"));
            } else {
                sanitized.push(word.to_owned());
            }
        } else {
            sanitized.push(word.to_owned());
        }
    }

    bounded_tail(&sanitized.join(" "), STDERR_EXCERPT_LIMIT)
}

fn diagnostic_assignment_key(word: &str) -> Option<&str> {
    let word = word.trim_matches(|character: char| {
        matches!(
            character,
            '\'' | '"' | '[' | ']' | '(' | ')' | '{' | '}' | ','
        )
    });
    let separator = word.find(['=', ':']).unwrap_or(word.len());
    let key = word[..separator].trim_start_matches('-');
    (!key.is_empty()).then_some(key)
}

fn diagnostic_assignment(word: &str) -> Option<(&str, &str)> {
    let word = word.trim_matches(|character: char| {
        matches!(
            character,
            '\'' | '"' | '[' | ']' | '(' | ')' | '{' | '}' | ',' | ';'
        )
    });
    if word.contains("://") {
        None
    } else {
        word.split_once(['=', ':'])
    }
}

fn is_sensitive_diagnostic_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace('-', "_");
    matches!(
        normalized.as_str(),
        "access_token"
            | "api_key"
            | "apikey"
            | "auth"
            | "authorization"
            | "bearer"
            | "cookie"
            | "cookie_file"
            | "cookies"
            | "cookies_from_browser"
            | "password"
            | "passwd"
            | "refresh_token"
            | "secret"
            | "set_cookie"
            | "signature"
            | "token"
            | "x_api_key"
            | "x_auth_token"
    )
}

fn looks_like_private_path(word: &str) -> bool {
    let word = word.trim_matches(|character: char| {
        matches!(
            character,
            '\'' | '"' | '[' | ']' | '(' | ')' | '{' | '}' | ',' | ';'
        )
    });
    let bytes = word.as_bytes();
    let windows_drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    let unc_path = word.starts_with("\\\\") || word.starts_with("//");
    let unix_path = word.starts_with('/') && word.len() > 1;
    windows_drive || unc_path || unix_path
}

fn redact_diagnostic_urls(value: &str) -> String {
    const SCHEMES: [&str; 5] = ["https://", "http://", "rtsps://", "rtsp://", "file://"];
    let mut result = String::with_capacity(value.len());
    let mut remaining = value;

    loop {
        let lowercase = remaining.to_ascii_lowercase();
        let Some((start, _)) = SCHEMES
            .iter()
            .filter_map(|scheme| lowercase.find(scheme).map(|index| (index, *scheme)))
            .min_by_key(|(index, _)| *index)
        else {
            result.push_str(remaining);
            break;
        };
        result.push_str(&remaining[..start]);
        let candidate = &remaining[start..];
        let end = candidate
            .char_indices()
            .find_map(|(index, character)| {
                (index > 0
                    && (character.is_whitespace()
                        || matches!(character, '\'' | '"' | '<' | '>' | '[' | ']' | '(' | ')')))
                .then_some(index)
            })
            .unwrap_or(candidate.len());
        let (url_text, rest) = candidate.split_at(end);
        result.push_str(&redacted_diagnostic_url(url_text));
        remaining = rest;
    }

    result
}

fn redacted_diagnostic_url(value: &str) -> String {
    let Ok(url) = reqwest::Url::parse(value.trim_end_matches(['.', ',', ';'])) else {
        return "[redacted URL]".to_owned();
    };
    if url.scheme() == "file" {
        return "[redacted URL]".to_owned();
    }
    let Some(host) = url.host_str() else {
        return "[redacted URL]".to_owned();
    };
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
}

fn bounded_tail(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut start = value.len().saturating_sub(limit);
    while !value.is_char_boundary(start) {
        start = start.saturating_add(1);
    }
    value[start..].to_owned()
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
    let command = command.with_seek("0");
    let mut process = platform_script_process(&command);
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
    context: RemoteStreamSpawnContext,
) -> Result<RemoteStreamProcess, RemoteStreamError> {
    let (arguments, producer, producer_seek) = prepare_remote_stream_spawn(command, producer);
    let mut producer_child = producer
        .as_ref()
        .map(|producer| spawn_remote_stream_producer(producer, &command.executable, None))
        .transpose()?;
    // ADR 0003: move an oversized graph out of argv before spawning, so a
    // complex channel cannot become unspawnable on a platform with a tight
    // command-line limit.
    let (arguments, filter_graph_file) = crate::filter_graph_file::apply_filter_graph_transport(
        &command.executable,
        &arguments,
        &crate::file_system_layout::filter_graph_folder(),
        false,
    );
    let mut process = Command::new(&command.executable);
    process
        .args(&arguments)
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
        let label = producer.as_ref().map_or_else(
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
            context,
            producer_seek,
        )?)
    } else {
        None
    };
    Ok(RemoteStreamProcess {
        child: consumer,
        producer_pipe,
        filter_graph_file,
    })
}

fn prepare_remote_stream_spawn(
    command: &FfmpegCommand,
    producer: Option<&RemoteStreamCommand>,
) -> (Vec<String>, Option<RemoteStreamCommand>, Option<String>) {
    let mut arguments = command.arguments.clone();
    let mut producer_seek = None;
    let producer = producer.map(|producer| {
        if producer.accepts_seek() {
            let seek_position =
                take_remote_stream_output_seek(&mut arguments).unwrap_or_else(|| "0".to_owned());
            producer_seek = Some(seek_position.clone());
            producer.with_seek(&seek_position)
        } else {
            producer.clone()
        }
    });
    (arguments, producer, producer_seek)
}

fn take_remote_stream_output_seek(arguments: &mut Vec<String>) -> Option<String> {
    arguments
        .windows(2)
        .any(|pair| pair == ["-i", "pipe:0"])
        .then_some(())?;
    let last_input_index = arguments
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| (pair[0] == "-i").then_some(index))
        .max()?;
    let seek_index = arguments
        .windows(2)
        .enumerate()
        .skip(last_input_index + 2)
        .find_map(|(index, pair)| (pair[0] == "-ss").then_some(index))?;
    let seek_position = arguments[seek_index + 1].clone();
    arguments.drain(seek_index..seek_index + 2);
    Some(seek_position)
}

pub(crate) fn pipe_remote_stream_producer(
    mut producer: Child,
    consumer_stdin: ChildStdin,
    label: String,
    context: RemoteStreamSpawnContext,
    effective_seek: Option<String>,
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
    let diagnostics = RemoteStreamProducerDiagnostics {
        label,
        context,
        effective_seek,
    };
    let task = tokio::spawn(run_remote_stream_producer(
        producer,
        stdout,
        consumer_stdin,
        stderr_task,
        cancellation,
        diagnostics,
    ));
    Ok(RemoteStreamProducerPipe {
        cancel: Some(cancel),
        task: Some(task),
    })
}

struct RemoteStreamProducerDiagnostics {
    label: String,
    context: RemoteStreamSpawnContext,
    effective_seek: Option<String>,
}

async fn run_remote_stream_producer(
    mut producer: Child,
    stdout: ChildStdout,
    consumer_stdin: ChildStdin,
    stderr_task: Option<JoinHandle<String>>,
    mut cancellation: oneshot::Receiver<()>,
    diagnostics: RemoteStreamProducerDiagnostics,
) {
    let RemoteStreamProducerDiagnostics {
        label,
        context,
        effective_seek,
    } = diagnostics;
    let copy_result = tokio::select! {
        result = copy_remote_producer_to_stdin(stdout, consumer_stdin) => Some(result),
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
        Ok(status)
            if status.success() && copy_result.as_ref().is_some_and(std::result::Result::is_ok) => {
        }
        Ok(status) if cancelled => tracing::debug!(
            producer = %label,
            %status,
            stderr_bytes = stderr.len(),
            "remote stream producer cancelled"
        ),
        Ok(status) if consumer_closed_remote_stream_pipe(copy_result.as_ref()) => tracing::debug!(
            producer = %label,
            %status,
            stderr_bytes = stderr.len(),
            copy_error = ?copy_result.as_ref().and_then(|result| result.as_ref().err()),
            "remote stream producer stopped after the consumer closed its input"
        ),
        Ok(status) => {
            let stderr_excerpt = sanitize_remote_stream_diagnostic(&stderr);
            tracing::warn!(
                surface = context.surface.as_str(),
                channel_number = context.channel_number.as_deref().unwrap_or("n/a"),
                producer = %label,
                seek = effective_seek.as_deref().unwrap_or("not-configured"),
                %status,
                failure_kind = "producer-exit",
                stderr_bytes = stderr.len(),
                stderr_excerpt = %stderr_excerpt,
                copy_error = ?copy_result.as_ref().and_then(|result| result.as_ref().err()),
                "remote stream producer stopped"
            );
        }
        Err(error) => {
            let stderr_excerpt = sanitize_remote_stream_diagnostic(&stderr);
            tracing::warn!(
                surface = context.surface.as_str(),
                channel_number = context.channel_number.as_deref().unwrap_or("n/a"),
                producer = %label,
                seek = effective_seek.as_deref().unwrap_or("not-configured"),
                %error,
                failure_kind = "producer-wait",
                stderr_bytes = stderr.len(),
                stderr_excerpt = %stderr_excerpt,
                "failed to wait for remote stream producer"
            );
        }
    }
}

fn consumer_closed_remote_stream_pipe(copy_result: Option<&io::Result<()>>) -> bool {
    copy_result.is_some_and(|result| {
        result.as_ref().is_err_and(|error| {
            matches!(
                error.kind(),
                io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::ConnectionReset
            )
        })
    })
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
    stdout: ChildStdout,
    consumer_stdin: tokio::process::ChildStdin,
) -> Result<(), RemoteStreamError> {
    copy_remote_producer_to_stdin(stdout, consumer_stdin)
        .await
        .map_err(|error| RemoteStreamError::Spawn(error.to_string()))
}

async fn copy_remote_producer_to_stdin(
    mut stdout: ChildStdout,
    mut consumer_stdin: tokio::process::ChildStdin,
) -> io::Result<()> {
    tokio::io::copy(&mut stdout, &mut consumer_stdin).await?;
    consumer_stdin.shutdown().await
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
    fn seek_placeholder_moves_non_seekable_output_seek_to_producer() {
        let producer = RemoteStreamCommand {
            program: "producer".to_owned(),
            args: vec![
                "https://example.test/watch".to_owned(),
                REMOTE_STREAM_SEEK_PLACEHOLDER.to_owned(),
            ],
            working_directory: PathBuf::from("."),
        };
        let consumer = FfmpegCommand {
            executable: "ffmpeg".to_owned(),
            environment: Vec::new(),
            arguments: vec![
                "-i".to_owned(),
                "pipe:0".to_owned(),
                "-ss".to_owned(),
                "00:00:05.0000000".to_owned(),
                "-i".to_owned(),
                "overlay.png".to_owned(),
                "-ss".to_owned(),
                "00:26:00.5000000".to_owned(),
                "-map".to_owned(),
                "0:v:0".to_owned(),
            ],
        };

        let (arguments, producer, effective_seek) =
            prepare_remote_stream_spawn(&consumer, Some(&producer));
        let producer = producer.expect("producer");

        assert_eq!(
            producer.args,
            ["https://example.test/watch", "00:26:00.5000000"]
        );
        assert_eq!(effective_seek.as_deref(), Some("00:26:00.5000000"));
        assert_eq!(
            arguments,
            [
                "-i",
                "pipe:0",
                "-ss",
                "00:00:05.0000000",
                "-i",
                "overlay.png",
                "-map",
                "0:v:0"
            ]
        );
    }

    #[test]
    fn unmarked_producer_keeps_consumer_seek_and_marked_zero_start_gets_zero() {
        let consumer = FfmpegCommand {
            executable: "ffmpeg".to_owned(),
            environment: Vec::new(),
            arguments: vec!["-i".to_owned(), "pipe:0".to_owned()],
        };
        let unmarked = RemoteStreamCommand {
            program: "producer".to_owned(),
            args: vec!["https://example.test/watch".to_owned()],
            working_directory: PathBuf::from("."),
        };
        let marked = RemoteStreamCommand {
            args: vec![
                "https://example.test/watch".to_owned(),
                REMOTE_STREAM_SEEK_PLACEHOLDER.to_owned(),
            ],
            ..unmarked.clone()
        };

        let (unmarked_arguments, unmarked, unmarked_seek) =
            prepare_remote_stream_spawn(&consumer, Some(&unmarked));
        let (marked_arguments, marked, marked_seek) =
            prepare_remote_stream_spawn(&consumer, Some(&marked));

        assert_eq!(unmarked_arguments, consumer.arguments);
        assert_eq!(unmarked_seek, None);
        assert_eq!(
            unmarked.expect("unmarked producer").args,
            ["https://example.test/watch"]
        );
        assert_eq!(marked_arguments, consumer.arguments);
        assert_eq!(marked_seek.as_deref(), Some("0"));
        assert_eq!(
            marked.expect("marked producer").args,
            ["https://example.test/watch", "0"]
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
        assert!(shell.contains("-ss \"$seek_position\""));
        assert!(batch.contains("-ss \"%SEEK_POSITION%\""));
    }

    #[test]
    fn youtube_extractor_uses_managed_ffmpeg_and_producer_seek() {
        let batch = include_str!("../../../assets/extractors/youtube/youtube.bat");
        let example =
            include_str!("../../../assets/extractors/youtube/youtube-windows.yml.example");

        assert!(batch.contains("if not defined FFMPEG_BIN"));
        assert!(batch.contains("YT_DLP_BIN"));
        assert!(batch.contains("best[ext=mp4][vcodec*=avc1][acodec*=mp4a]"));
        assert!(batch.contains("--ffmpeg-location \"%FFMPEG_BIN%\""));
        assert!(batch.contains("--downloader ffmpeg"));
        assert!(batch.contains("--downloader-args \"ffmpeg_i:-ss %SEEK_POSITION%\""));
        assert!(batch.contains("--hls-use-mpegts"));
        assert!(batch.contains("--output -"));
        assert!(batch.contains("--quiet"));
        assert!(!batch.contains("--no-warnings"));
        assert!(!batch.contains("--no-progress"));
        assert!(example.contains("\"{seek}\""));

        let definition = parse_remote_stream_definition(example).expect("example definition");
        assert!(definition.url.is_none());
        assert!(!definition.is_live);
        assert_eq!(definition.duration.as_deref(), Some("00:20:00"));
        let resolved = resolve_remote_stream(
            &definition,
            Path::new("assets/extractors/youtube/youtube-windows.yml"),
        )
        .expect("example source");
        let producer = remote_stream_producer(&resolved).expect("script producer");
        assert_eq!(
            producer.args,
            [
                "https://www.youtube.com/watch?v=<video-id>",
                REMOTE_STREAM_SEEK_PLACEHOLDER,
            ]
        );
        assert!(producer.accepts_seek());
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

    #[test]
    fn sanitizes_signed_urls_secrets_paths_and_multiline_output() {
        let stderr = concat!(
            "ERROR: source https://user:placeholder@example.test/media/file.m3u8?sig=placeholder#part\n",
            "Authorization: Bearer placeholder cookie=placeholder ",
            "X-API-Key: placeholder Set-Cookie: placeholder ",
            r"X:\private\cookies.txt /tmp cache=/srv/cache/item file:///srv/private/cookies.txt",
        );

        let sanitized = sanitize_remote_stream_diagnostic(stderr);

        assert!(sanitized.contains("ERROR: source https://example.test"));
        assert!(sanitized.contains("authorization=[redacted] [redacted] [redacted]"));
        assert!(sanitized.contains("cookie=[redacted]"));
        assert!(sanitized.contains("x-api-key=[redacted]"));
        assert!(sanitized.contains("set-cookie=[redacted]"));
        assert!(sanitized.contains("[redacted path]"));
        assert!(sanitized.contains("cache=[redacted path]"));
        assert!(sanitized.contains("[redacted URL]"));
        assert!(!sanitized.contains("placeholder"));
        assert!(!sanitized.contains("private"));
        assert!(!sanitized.contains('\n'));
    }

    #[test]
    fn diagnostic_excerpt_is_a_bounded_utf8_tail() {
        let stderr = format!("{}final error �", "x".repeat(STDERR_EXCERPT_LIMIT * 2));

        let sanitized = sanitize_remote_stream_diagnostic(&stderr);

        assert!(sanitized.len() <= STDERR_EXCERPT_LIMIT);
        assert!(sanitized.ends_with("final error �"));
    }

    #[test]
    fn only_consumer_disconnect_errors_are_expected_pipe_teardown() {
        for kind in [
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::ConnectionReset,
        ] {
            let result = Some(Err(io::Error::from(kind)));
            assert!(
                consumer_closed_remote_stream_pipe(result.as_ref()),
                "{kind:?}"
            );
        }

        for kind in [
            io::ErrorKind::InvalidData,
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::UnexpectedEof,
        ] {
            let result = Some(Err(io::Error::from(kind)));
            assert!(
                !consumer_closed_remote_stream_pipe(result.as_ref()),
                "{kind:?}"
            );
        }
        assert!(!consumer_closed_remote_stream_pipe(Some(&Ok(()))));
        assert!(!consumer_closed_remote_stream_pipe(None));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bounded_consumer_close_is_detected_from_a_real_process_pipe() {
        let mut producer = Command::new("sh");
        producer
            .args(["-c", "while :; do printf remote-bytes; done"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut producer = producer.spawn().expect("spawn producer");
        let producer_stdout = producer.stdout.take().expect("producer stdout");

        let mut consumer = Command::new("sh");
        consumer
            .args(["-c", "head -c 1 >/dev/null"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut consumer = consumer.spawn().expect("spawn bounded consumer");
        let consumer_stdin = consumer.stdin.take().expect("consumer stdin");

        let copy_result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            copy_remote_producer_to_stdin(producer_stdout, consumer_stdin),
        )
        .await
        .expect("producer pipe should observe the bounded consumer close");

        assert!(consumer_closed_remote_stream_pipe(Some(&copy_result)));
        assert!(consumer.wait().await.expect("consumer status").success());
        let _ = producer.start_kill();
        let _ = producer.wait().await;
    }

    #[tokio::test]
    async fn pipes_producer_stdout_directly_to_consumer_stdin() {
        use tokio::io::AsyncReadExt as _;

        let (producer, consumer) = producer_and_consumer();
        let mut process = spawn_ffmpeg_with_remote_input(
            &consumer,
            None,
            Some(&producer),
            RemoteStreamSpawnContext::probe(),
        )
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
        let mut process = spawn_ffmpeg_with_remote_input(
            &consumer,
            None,
            Some(&producer),
            RemoteStreamSpawnContext::probe(),
        )
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
