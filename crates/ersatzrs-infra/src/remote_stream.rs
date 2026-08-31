use std::collections::{BTreeMap, VecDeque};
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

pub use ersatzrs_addon_contract::RemoteStreamPerson;
use ersatzrs_addon_contract::{
    validate_addon_id, AddonOperationError, MAX_OPERATION_ERROR_MESSAGE_BYTES,
    OPERATION_CODE_FAILED,
};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, RemoteStreamLiveness,
};
use ersatzrs_ffmpeg::{FfmpegCommand, InputFile};

use crate::addons::{
    spawn_managed_addon_process, AddonRuntimeContext, ManagedAddonChild, ResolvedAddonCommand,
};
use crate::db::DbPool;

const REMOTE_INPUT_TIMEOUT_MICROSECONDS: &str = "30000000";
const REMOTE_STREAM_SEEK_PLACEHOLDER: &str = "{seek}";
/// Add-on play v2 environment variable carrying a maximum video height.
pub const REMOTE_STREAM_MAX_VIDEO_HEIGHT_ENV: &str = "ERSATZRS_REMOTE_STREAM_MAX_VIDEO_HEIGHT";
/// Initial live add-on video-height ceiling.
pub const REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT: u16 = 1080;
/// Bounded live add-on fallback video-height ceiling.
pub const REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT: u16 = 720;
const STDERR_TAIL_LIMIT: usize = 16 * 1024;
const STDERR_EXCERPT_LIMIT: usize = 2 * 1024;
const STDERR_READER_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
const FILE_SECRET_DIAGNOSTIC: &str =
    "add-on diagnostics unavailable while a protected file reference is configured";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteStreamDefinition {
    /// Stable provider item identity used to correlate list and stream records.
    pub provider_id: Option<String>,
    /// Stable provider identity of the full stream for chapter fragments.
    pub parent_provider_id: Option<String>,
    pub fragment_start_seconds: Option<u64>,
    pub fragment_end_seconds: Option<u64>,
    pub url: Option<String>,
    pub addon: Option<String>,
    pub duration: Option<String>,
    pub fallback_query: Option<String>,
    /// Authoritative provider-neutral playback behavior. `None` is retained
    /// only while adapting a legacy definition.
    #[serde(default)]
    pub liveness: Option<RemoteStreamLiveness>,
    /// Compatibility mirror for legacy YAML and callers.
    #[serde(default)]
    pub is_live: bool,
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub outline: Option<String>,
    pub tagline: Option<String>,
    pub show_title: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub plot: Option<String>,
    pub year: Option<i32>,
    /// Original publication/broadcast instant in RFC 3339 form.
    pub release_date: Option<String>,
    pub content_rating: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub collection: Option<String>,
    #[serde(default)]
    pub people: Vec<RemoteStreamPerson>,
    #[serde(default)]
    pub producers: Vec<String>,
    #[serde(default)]
    pub original_broadcasters: Vec<String>,
    #[serde(default)]
    pub broadcasters: Vec<String>,
    #[serde(default)]
    pub studios: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub guids: Vec<String>,
    #[serde(default)]
    pub artwork_urls: Vec<String>,
    #[serde(default)]
    pub availability: AddonMediaListItemAvailability,
    pub availability_reason: Option<String>,
    #[serde(default)]
    pub content_kind: AddonMediaListContentKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteStreamCommand {
    pub program: String,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub environment: BTreeMap<String, String>,
}

impl RemoteStreamCommand {
    fn accepts_seek(&self) -> bool {
        self.args
            .iter()
            .any(|argument| argument == REMOTE_STREAM_SEEK_PLACEHOLDER)
            || self
                .environment
                .values()
                .any(|value| value == REMOTE_STREAM_SEEK_PLACEHOLDER)
    }

    fn with_seek(&self, seek_position: &str) -> Self {
        let mut command = self.clone();
        for argument in &mut command.args {
            if argument == REMOTE_STREAM_SEEK_PLACEHOLDER {
                seek_position.clone_into(argument);
            }
        }
        for value in command.environment.values_mut() {
            if value == REMOTE_STREAM_SEEK_PLACEHOLDER {
                seek_position.clone_into(value);
            }
        }
        command
    }

    fn supports_video_height_ceiling(&self) -> bool {
        self.environment
            .contains_key(REMOTE_STREAM_MAX_VIDEO_HEIGHT_ENV)
    }

    fn with_video_height_ceiling(&self, max_video_height: u16) -> Self {
        let mut command = self.clone();
        if command.supports_video_height_ceiling() {
            command.environment.insert(
                REMOTE_STREAM_MAX_VIDEO_HEIGHT_ENV.to_owned(),
                max_video_height.to_string(),
            );
        }
        command
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteStreamSource {
    Url(String),
    Addon(RemoteStreamCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRemoteStream {
    pub source: RemoteStreamSource,
    pub liveness: RemoteStreamLiveness,
    /// Compatibility projection; true only when `liveness` is live.
    pub is_live: bool,
    pub configured_duration: Option<String>,
    pub fallback_query: Option<String>,
    pub uses_addon: bool,
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
    #[error("remote stream add-on command is invalid: {0}")]
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
    let raw: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|error| RemoteStreamError::Definition(error.to_string()))?;
    let legacy_was_explicit = raw
        .as_mapping()
        .is_some_and(|mapping| mapping.contains_key("is_live"));
    let mut definition: RemoteStreamDefinition = serde_yaml::from_value(raw)
        .map_err(|error| RemoteStreamError::Definition(error.to_string()))?;
    if let Some(liveness) = definition.liveness {
        if legacy_was_explicit && definition.is_live != liveness.legacy_is_live() {
            return Err(RemoteStreamError::Definition(
                "liveness contradicts legacy is_live".to_owned(),
            ));
        }
    } else if legacy_was_explicit {
        definition.liveness = Some(RemoteStreamLiveness::from_legacy(
            Some(definition.is_live),
            None,
        ));
    }
    let liveness = definition.effective_liveness()?;
    definition.is_live = liveness.legacy_is_live();
    validate_definition(&definition)?;
    Ok(definition)
}

impl RemoteStreamDefinition {
    /// Resolve the new discriminator or conservatively adapt a legacy row.
    ///
    /// # Errors
    ///
    /// Returns an error when a configured duration cannot be decoded.
    pub fn effective_liveness(&self) -> Result<RemoteStreamLiveness, RemoteStreamError> {
        if let Some(liveness) = self.liveness {
            return Ok(liveness);
        }
        if self.is_live {
            return Ok(RemoteStreamLiveness::Live);
        }
        let has_positive_duration = self
            .duration
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(duration_is_positive)
            .transpose()?
            .unwrap_or(false);
        Ok(if has_positive_duration {
            RemoteStreamLiveness::Finite
        } else {
            RemoteStreamLiveness::Unknown
        })
    }
}

/// Decode the authoritative database discriminator and verify its legacy
/// `IsLive` projection.
///
/// # Errors
///
/// Returns an error for an unknown discriminator or a drifting mirror.
pub fn decode_persisted_liveness(
    value: i64,
    legacy_is_live: bool,
) -> Result<RemoteStreamLiveness, RemoteStreamError> {
    let liveness = match value {
        0 => RemoteStreamLiveness::Unknown,
        1 => RemoteStreamLiveness::Finite,
        2 => RemoteStreamLiveness::Live,
        _ => {
            return Err(RemoteStreamError::Definition(format!(
                "stored liveness discriminator {value} is invalid"
            )))
        }
    };
    if liveness.legacy_is_live() != legacy_is_live {
        return Err(RemoteStreamError::Definition(
            "stored liveness and legacy IsLive mirror disagree".to_owned(),
        ));
    }
    Ok(liveness)
}

/// Resolve a direct URL source.
///
/// # Errors
///
/// Returns an error when the definition is invalid or requires an add-on.
pub fn resolve_remote_stream(
    definition: &RemoteStreamDefinition,
    _definition_path: &Path,
) -> Result<ResolvedRemoteStream, RemoteStreamError> {
    validate_definition(definition)?;
    if trimmed(definition.addon.as_ref()).is_some() {
        return Err(RemoteStreamError::Definition(
            "add-on source requires the installed add-on resolver".to_owned(),
        ));
    }
    resolve_remote_stream_source(definition, None, false)
}

/// Resolve a URL through an installed, enabled add-on when `addon:` is set.
///
/// # Errors
///
/// Returns an error when the definition or installed add-on is unavailable.
pub async fn resolve_remote_stream_with_addons(
    pool: &DbPool,
    definition: &RemoteStreamDefinition,
    runtime: &AddonRuntimeContext,
) -> Result<ResolvedRemoteStream, RemoteStreamError> {
    resolve_remote_stream_with_addons_mode(pool, definition, runtime, false).await
}

/// Resolve a source for scanner persistence without admitting unknown
/// liveness to playback.
///
/// # Errors
///
/// Returns an error when the definition or installed add-on is unavailable.
pub async fn resolve_remote_stream_for_scan_with_addons(
    pool: &DbPool,
    definition: &RemoteStreamDefinition,
    runtime: &AddonRuntimeContext,
) -> Result<ResolvedRemoteStream, RemoteStreamError> {
    resolve_remote_stream_with_addons_mode(pool, definition, runtime, true).await
}

async fn resolve_remote_stream_with_addons_mode(
    pool: &DbPool,
    definition: &RemoteStreamDefinition,
    runtime: &AddonRuntimeContext,
    allow_unknown: bool,
) -> Result<ResolvedRemoteStream, RemoteStreamError> {
    validate_definition(definition)?;
    let command = if let Some(addon_id) = trimmed(definition.addon.as_ref()) {
        let installation = crate::addons::load_installed_addon(pool, addon_id)
            .await
            .map_err(|error| RemoteStreamError::Command(error.to_string()))?
            .ok_or_else(|| {
                RemoteStreamError::Command(format!("add-on '{addon_id}' is not installed"))
            })?;
        let play_capability = if installation
            .manifest
            .has_capability(ersatzrs_addon_contract::CAPABILITY_REMOTE_STREAM_PLAY_V2)
        {
            ersatzrs_addon_contract::CAPABILITY_REMOTE_STREAM_PLAY_V2
        } else {
            ersatzrs_addon_contract::CAPABILITY_REMOTE_STREAM_PLAY_V1
        };
        let resolved = crate::addons::resolve_enabled_addon_command(
            pool,
            addon_id,
            play_capability,
            "play",
            runtime,
        )
        .await
        .map_err(|error| RemoteStreamError::Command(error.to_string()))?;
        Some(remote_stream_command_from_addon(
            resolved,
            trimmed(definition.url.as_ref()).unwrap_or_default(),
            play_capability == ersatzrs_addon_contract::CAPABILITY_REMOTE_STREAM_PLAY_V2,
        ))
    } else {
        None
    };
    resolve_remote_stream_source(definition, command, allow_unknown)
}

fn resolve_remote_stream_source(
    definition: &RemoteStreamDefinition,
    addon_command: Option<RemoteStreamCommand>,
    allow_unknown: bool,
) -> Result<ResolvedRemoteStream, RemoteStreamError> {
    let url = trimmed(definition.url.as_ref())
        .ok_or_else(|| RemoteStreamError::Definition("url is required".to_owned()))?;
    let uses_addon = addon_command.is_some();
    let source = addon_command.map_or_else(
        || RemoteStreamSource::Url(url.to_owned()),
        RemoteStreamSource::Addon,
    );
    let liveness = definition.effective_liveness()?;
    if liveness == RemoteStreamLiveness::Unknown && !allow_unknown {
        return Err(RemoteStreamError::Definition(
            "playback behavior is unknown; classify the source as finite or live before playback"
                .to_owned(),
        ));
    }
    Ok(ResolvedRemoteStream {
        source,
        liveness,
        is_live: liveness.legacy_is_live(),
        configured_duration: definition.duration.clone(),
        fallback_query: definition.fallback_query.clone(),
        uses_addon,
    })
}

fn remote_stream_command_from_addon(
    resolved: ResolvedAddonCommand,
    url: &str,
    supports_video_height_ceiling: bool,
) -> RemoteStreamCommand {
    let mut environment = resolved.environment;
    environment.insert("ERSATZRS_REMOTE_STREAM_URL".to_owned(), url.to_owned());
    environment.insert(
        "ERSATZRS_REMOTE_STREAM_SEEK".to_owned(),
        REMOTE_STREAM_SEEK_PLACEHOLDER.to_owned(),
    );
    if supports_video_height_ceiling {
        environment.insert(
            REMOTE_STREAM_MAX_VIDEO_HEIGHT_ENV.to_owned(),
            REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT.to_string(),
        );
    }
    RemoteStreamCommand {
        program: resolved.program,
        args: resolved.args,
        working_directory: resolved.working_directory,
        environment,
    }
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
            let input = InputFile::video(url.clone()).with_input_options(remote_url_input_options(
                url,
                stream.liveness == RemoteStreamLiveness::Live,
            ));
            if stream.liveness == RemoteStreamLiveness::Live {
                input.without_readrate()
            } else {
                input
            }
        }
        RemoteStreamSource::Addon(_) => {
            // ErsatzTV forwards producer stdout through its internal endpoint
            // without adding an FFmpeg input format. Keep the direct pipe
            // equally opaque so FFmpeg can probe streamable MP4, WebM, MPEG-TS,
            // and other supported containers from their bytes.
            let input = InputFile::video("pipe:0").with_seek_after_input();
            if stream.liveness == RemoteStreamLiveness::Live {
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
        RemoteStreamSource::Addon(command) => Some(command),
        RemoteStreamSource::Url(_) => None,
    }
}

#[must_use]
pub fn remote_stream_video_height_ceiling(stream: &ResolvedRemoteStream) -> Option<u16> {
    let RemoteStreamSource::Addon(command) = &stream.source else {
        return None;
    };
    command
        .environment
        .get(REMOTE_STREAM_MAX_VIDEO_HEIGHT_ENV)
        .and_then(|value| value.parse::<u16>().ok())
}

#[must_use]
pub fn with_remote_stream_video_height_ceiling(
    stream: &ResolvedRemoteStream,
    max_video_height: u16,
) -> ResolvedRemoteStream {
    let mut stream = stream.clone();
    if let RemoteStreamSource::Addon(command) = &mut stream.source {
        *command = command.with_video_height_ceiling(max_video_height);
    }
    stream
}

/// Keep finite sources seekable while always opening a live source at its
/// current producer boundary.
#[must_use]
pub fn remote_stream_playback_start(
    stream: Option<&ResolvedRemoteStream>,
    requested: std::time::Duration,
) -> std::time::Duration {
    if stream.is_some_and(|stream| stream.liveness == RemoteStreamLiveness::Live) {
        std::time::Duration::ZERO
    } else {
        requested
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
        RemoteStreamSource::Addon(command) => Path::new(&command.program)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("add-on")
            .to_owned(),
    }
}

#[must_use]
pub fn redact_remote_stream_output(value: &str, stream: &ResolvedRemoteStream) -> String {
    let source_redacted = match &stream.source {
        RemoteStreamSource::Url(url) => value.replace(url, &redacted_source_label(&stream.source)),
        RemoteStreamSource::Addon(command) if command_has_file_backed_secret(command) => {
            FILE_SECRET_DIAGNOSTIC.to_owned()
        }
        RemoteStreamSource::Addon(command) => {
            redact_exact_secrets(value, &remote_stream_environment_secrets(command))
        }
    };
    sanitize_remote_stream_diagnostic(&source_redacted)
}

pub(crate) fn command_has_file_backed_secret(command: &RemoteStreamCommand) -> bool {
    command
        .environment
        .keys()
        .any(|name| name.starts_with("ERSATZRS_ADDON_SECRET_FILE_"))
}

pub(crate) fn remote_stream_environment_secrets(command: &RemoteStreamCommand) -> Vec<String> {
    let mut secrets = command
        .environment
        .iter()
        .filter(|(name, value)| {
            (name.starts_with("ERSATZRS_ADDON_SECRET_")
                || (name.starts_with("ERSATZRS_") && name.ends_with("_URL")))
                && !value.is_empty()
        })
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    secrets.sort_unstable_by_key(|secret| std::cmp::Reverse(secret.len()));
    secrets.dedup();
    secrets
}

fn redact_exact_secrets(value: &str, secrets: &[String]) -> String {
    secrets.iter().fold(value.to_owned(), |redacted, secret| {
        redacted.replace(secret, "[redacted]")
    })
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
    const SCHEMES: [&str; 11] = [
        "https://", "http://", "rtsps://", "rtsp://", "rtmps://", "rtmp://", "srt://", "rist://",
        "udp://", "tcp://", "file://",
    ];
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

/// Start a configured Remote Stream add-on producer.
///
/// # Errors
///
/// Returns an error when the producer process cannot be started.
pub(crate) fn spawn_remote_stream_producer(
    command: &RemoteStreamCommand,
) -> Result<ManagedAddonChild, RemoteStreamError> {
    let command = command.with_seek("0");
    let mut process = platform_script_process(&command);
    process
        .current_dir(&command.working_directory)
        .env_clear()
        .envs(crate::addons::addon_process_environment())
        .envs(&command.environment)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    spawn_managed_addon_process(process)
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
        .map(spawn_remote_stream_producer)
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
            || "add-on".to_owned(),
            |producer| {
                Path::new(&producer.program)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("add-on")
                    .to_owned()
            },
        );
        let secrets = producer
            .as_ref()
            .map(remote_stream_environment_secrets)
            .unwrap_or_default();
        let suppress_diagnostics = producer
            .as_ref()
            .is_some_and(command_has_file_backed_secret);
        Some(pipe_remote_stream_producer(
            producer_child,
            consumer_stdin,
            label,
            context,
            producer_seek,
            secrets,
            suppress_diagnostics,
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
    mut producer: ManagedAddonChild,
    consumer_stdin: ChildStdin,
    label: String,
    context: RemoteStreamSpawnContext,
    effective_seek: Option<String>,
    secrets: Vec<String>,
    suppress_diagnostics: bool,
) -> Result<RemoteStreamProducerPipe, RemoteStreamError> {
    let stdout = producer
        .stdout()
        .take()
        .ok_or(RemoteStreamError::MissingStdout)?;
    let stderr_task = producer
        .stderr()
        .take()
        .map(|stderr| tokio::spawn(read_stderr_tail(stderr)));
    let (cancel, cancellation) = oneshot::channel();
    let diagnostics = RemoteStreamProducerDiagnostics {
        label,
        context,
        effective_seek,
        secrets,
        suppress_diagnostics,
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
    secrets: Vec<String>,
    suppress_diagnostics: bool,
}

async fn run_remote_stream_producer(
    mut producer: ManagedAddonChild,
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
        secrets,
        suppress_diagnostics,
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
    let _ = producer.start_kill();
    let stderr = match stderr_task {
        Some(task) => finish_stderr_task(task).await,
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
            let diagnostic = producer_operation_diagnostic(&stderr, &secrets, suppress_diagnostics);
            tracing::warn!(
                surface = context.surface.as_str(),
                channel_number = context.channel_number.as_deref().unwrap_or("n/a"),
                producer = %label,
                seek = effective_seek.as_deref().unwrap_or("not-configured"),
                %status,
                failure_kind = "producer-exit",
                stderr_bytes = stderr.len(),
                operation_code = %diagnostic.code,
                operation_message = %diagnostic.message,
                copy_error = ?copy_result.as_ref().and_then(|result| result.as_ref().err()),
                "remote stream producer stopped"
            );
        }
        Err(error) => {
            let diagnostic = producer_operation_diagnostic(&stderr, &secrets, suppress_diagnostics);
            tracing::warn!(
                surface = context.surface.as_str(),
                channel_number = context.channel_number.as_deref().unwrap_or("n/a"),
                producer = %label,
                seek = effective_seek.as_deref().unwrap_or("not-configured"),
                %error,
                failure_kind = "producer-wait",
                stderr_bytes = stderr.len(),
                operation_code = %diagnostic.code,
                operation_message = %diagnostic.message,
                "failed to wait for remote stream producer"
            );
        }
    }
}

async fn finish_stderr_task(mut task: JoinHandle<String>) -> String {
    if let Ok(result) = tokio::time::timeout(STDERR_READER_SHUTDOWN_TIMEOUT, &mut task).await {
        result.unwrap_or_default()
    } else {
        task.abort();
        let _ = task.await;
        String::new()
    }
}

fn producer_operation_diagnostic(
    stderr: &str,
    secrets: &[String],
    suppress_diagnostics: bool,
) -> AddonOperationError {
    if let Some(line) = stderr.lines().rev().find(|line| !line.trim().is_empty()) {
        if let Ok(diagnostic) = serde_json::from_str::<AddonOperationError>(line.trim()) {
            if diagnostic.validate().is_ok() {
                let message = if suppress_diagnostics {
                    FILE_SECRET_DIAGNOSTIC.to_owned()
                } else {
                    sanitize_remote_stream_diagnostic(&redact_exact_secrets(
                        &diagnostic.message,
                        secrets,
                    ))
                };
                return AddonOperationError::new(&diagnostic.code, message);
            }
        }
    }
    let message = if suppress_diagnostics {
        FILE_SECRET_DIAGNOSTIC.to_owned()
    } else {
        bounded_tail(
            &sanitize_remote_stream_diagnostic(&redact_exact_secrets(stderr, secrets)),
            MAX_OPERATION_ERROR_MESSAGE_BYTES,
        )
    };
    AddonOperationError::new(OPERATION_CODE_FAILED, message)
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
    if trimmed(definition.url.as_ref()).is_none() {
        return Err(RemoteStreamError::Definition("url is required".to_owned()));
    }
    if definition
        .addon
        .as_deref()
        .is_some_and(|addon| validate_addon_id(addon).is_err())
    {
        return Err(RemoteStreamError::Definition(
            "addon must be a reverse-DNS identifier".to_owned(),
        ));
    }
    if let Some(duration) = trimmed(definition.duration.as_ref()) {
        validate_duration(duration)?;
    }
    if let Some(release_date) = trimmed(definition.release_date.as_ref()) {
        chrono::DateTime::parse_from_rfc3339(release_date).map_err(|_| {
            RemoteStreamError::Definition(
                "release_date must be an RFC 3339 date and time".to_owned(),
            )
        })?;
    }
    for person in &definition.people {
        if person.name.trim().is_empty() || person.role.trim().is_empty() {
            return Err(RemoteStreamError::Definition(
                "people entries require non-empty name and role values".to_owned(),
            ));
        }
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

fn duration_is_positive(value: &str) -> Result<bool, RemoteStreamError> {
    validate_duration(value)?;
    let mut parts = value.split(':');
    let hours = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default();
    let minutes = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default();
    let seconds = parts.next().unwrap_or_default();
    let (seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    Ok(hours > 0
        || minutes > 0
        || seconds.parse::<u64>().unwrap_or_default() > 0
        || fraction.chars().any(|character| character != '0'))
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
                environment: BTreeMap::new(),
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
                environment: BTreeMap::new(),
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
                environment: BTreeMap::new(),
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
                environment: BTreeMap::new(),
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
    fn parses_complete_direct_definition() {
        let definition = parse_remote_stream_definition(
            r"
url: https://user:secret@example.test/live.m3u8?token=hidden
is_live: true
duration: 03:41:00
title: Example
plot: Plot
year: 1994
content_rating: PG
",
        )
        .expect("definition");
        let resolved =
            resolve_remote_stream(&definition, Path::new("streams/example.yml")).expect("source");
        assert!(!resolved.uses_addon);
        assert!(resolved.is_live);
        assert_eq!(
            redacted_source_label(&resolved.source),
            "https://example.test"
        );
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
            environment: BTreeMap::new(),
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
            environment: BTreeMap::new(),
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
            environment: BTreeMap::new(),
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
    fn video_height_ceiling_is_applied_only_to_v2_addon_commands() {
        let v1 = ResolvedRemoteStream {
            source: RemoteStreamSource::Addon(RemoteStreamCommand {
                program: "producer".to_owned(),
                args: Vec::new(),
                working_directory: PathBuf::from("."),
                environment: BTreeMap::new(),
            }),
            liveness: RemoteStreamLiveness::Live,
            is_live: true,
            configured_duration: None,
            fallback_query: None,
            uses_addon: true,
        };
        let mut v2 = v1.clone();
        let RemoteStreamSource::Addon(command) = &mut v2.source else {
            unreachable!("fixture uses an add-on command");
        };
        command.environment.insert(
            REMOTE_STREAM_MAX_VIDEO_HEIGHT_ENV.to_owned(),
            REMOTE_STREAM_DEFAULT_MAX_VIDEO_HEIGHT.to_string(),
        );

        let unchanged_v1 =
            with_remote_stream_video_height_ceiling(&v1, REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT);
        let capped_v2 =
            with_remote_stream_video_height_ceiling(&v2, REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT);

        assert_eq!(remote_stream_video_height_ceiling(&unchanged_v1), None);
        assert_eq!(
            remote_stream_video_height_ceiling(&capped_v2),
            Some(REMOTE_STREAM_FALLBACK_MAX_VIDEO_HEIGHT)
        );
    }

    #[test]
    fn add_on_seek_placeholder_is_projected_through_environment() {
        let command = RemoteStreamCommand {
            program: "producer".to_owned(),
            args: vec!["play".to_owned()],
            working_directory: PathBuf::from("."),
            environment: BTreeMap::from([(
                "ERSATZRS_REMOTE_STREAM_SEEK".to_owned(),
                REMOTE_STREAM_SEEK_PLACEHOLDER.to_owned(),
            )]),
        };

        assert!(command.accepts_seek());
        assert_eq!(
            command
                .with_seek("00:20:00")
                .environment
                .get("ERSATZRS_REMOTE_STREAM_SEEK")
                .map(String::as_str),
            Some("00:20:00")
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn launches_windows_batch_producer_with_raw_cmd_arguments() {
        let root = tempfile::Builder::new()
            .prefix("remote stream producer ")
            .tempdir()
            .expect("temp directory");
        let batch = root.path().join("emit arguments.bat");
        std::fs::write(
            &batch,
            "@echo off\r\n<nul set /p=\"%~1|%~2\"\r\nexit /b 0\r\n",
        )
        .expect("write batch producer");
        let command = RemoteStreamCommand {
            program: batch.to_string_lossy().into_owned(),
            args: vec![
                "https://example.test/watch?id=1&x=2".to_owned(),
                String::new(),
            ],
            working_directory: root.path().to_path_buf(),
            environment: BTreeMap::new(),
        };

        let mut child = spawn_remote_stream_producer(&command).expect("spawn batch producer");
        let mut stdout = child.stdout().take().expect("producer stdout");
        let mut stderr = child.stderr().take().expect("producer stderr");
        let mut output = String::new();
        let mut error_output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .expect("read producer output");
        stderr
            .read_to_string(&mut error_output)
            .await
            .expect("read producer error output");
        let status = child.wait().await.expect("producer status");

        assert!(status.success(), "producer failed: {error_output}");
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
    fn authoritative_liveness_accepts_an_optional_compatible_legacy_mirror() {
        let without_mirror =
            parse_remote_stream_definition("url: https://example.test\nliveness: live")
                .expect("authoritative liveness without legacy mirror");
        assert_eq!(without_mirror.liveness, Some(RemoteStreamLiveness::Live));
        assert!(without_mirror.is_live);

        let unknown = parse_remote_stream_definition(
            "url: https://example.test\nliveness: unknown\nis_live: false",
        )
        .expect("unknown liveness has a false compatibility mirror");
        assert_eq!(unknown.liveness, Some(RemoteStreamLiveness::Unknown));
        assert!(!unknown.is_live);

        assert!(parse_remote_stream_definition(
            "url: https://example.test\nliveness: finite\nis_live: true"
        )
        .expect_err("an unequivocally contradictory mirror must fail")
        .to_string()
        .contains("contradicts"));
    }

    #[test]
    fn explicit_live_source_and_http_live_reconnect_at_eof() {
        let rtsp = RemoteStreamDefinition {
            url: Some("rtsp://camera.test/live".to_owned()),
            addon: None,
            duration: None,
            fallback_query: None,
            liveness: Some(RemoteStreamLiveness::Live),
            is_live: false,
            title: None,
            plot: None,
            year: None,
            content_rating: None,
            tags: Vec::new(),
            ..RemoteStreamDefinition::default()
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
    fn add_on_input_uses_container_autodetection() {
        let stream = ResolvedRemoteStream {
            source: RemoteStreamSource::Addon(RemoteStreamCommand {
                program: "producer".to_owned(),
                args: Vec::new(),
                working_directory: PathBuf::from("streams"),
                environment: BTreeMap::new(),
            }),
            liveness: RemoteStreamLiveness::Finite,
            is_live: false,
            configured_duration: None,
            fallback_query: None,
            uses_addon: true,
        };

        let input = remote_stream_input_file(&stream);

        assert_eq!(input.path, "pipe:0");
        assert!(input.input_options.is_empty());
        assert!(input.seek_after_input);
        assert!(input.allow_readrate);
    }

    #[test]
    fn live_playback_starts_at_zero_without_input_readrate() {
        let requested = std::time::Duration::from_secs(47);
        let live = ResolvedRemoteStream {
            source: RemoteStreamSource::Url("https://example.test/live.m3u8".to_owned()),
            liveness: RemoteStreamLiveness::Live,
            is_live: true,
            configured_duration: None,
            fallback_query: None,
            uses_addon: false,
        };
        let finite = ResolvedRemoteStream {
            liveness: RemoteStreamLiveness::Finite,
            is_live: false,
            ..live.clone()
        };

        assert_eq!(
            remote_stream_playback_start(Some(&live), requested),
            std::time::Duration::ZERO
        );
        assert_eq!(
            remote_stream_playback_start(Some(&finite), requested),
            requested
        );
        assert!(!remote_stream_input_file(&live).allow_readrate);
        assert!(remote_stream_input_file(&finite).allow_readrate);
    }

    #[test]
    fn redacts_credentials_and_query_parameters_from_ffmpeg_output() {
        let stream = ResolvedRemoteStream {
            source: RemoteStreamSource::Url(
                "https://user:secret@example.test/live.m3u8?token=hidden".to_owned(),
            ),
            liveness: RemoteStreamLiveness::Live,
            is_live: true,
            configured_duration: None,
            fallback_query: None,
            uses_addon: false,
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
    fn redacts_bare_add_on_environment_secrets_from_output() {
        let stream = ResolvedRemoteStream {
            source: RemoteStreamSource::Addon(RemoteStreamCommand {
                program: "producer".to_owned(),
                args: Vec::new(),
                working_directory: PathBuf::from("streams"),
                environment: BTreeMap::from([
                    (
                        "ERSATZRS_ADDON_SECRET_API_TOKEN".to_owned(),
                        "long-secret-value".to_owned(),
                    ),
                    (
                        "ERSATZRS_ADDON_SETTING_LABEL".to_owned(),
                        "visible-setting".to_owned(),
                    ),
                ]),
            }),
            liveness: RemoteStreamLiveness::Finite,
            is_live: false,
            configured_duration: None,
            fallback_query: None,
            uses_addon: true,
        };

        let stderr = redact_remote_stream_output(
            "producer failed with long-secret-value for visible-setting",
            &stream,
        );

        assert_eq!(
            stderr,
            "producer failed with [redacted] for visible-setting"
        );
        assert!(!stderr.contains("long-secret-value"));
    }

    #[test]
    fn redacts_operation_urls_and_suppresses_file_secret_diagnostics() {
        let operation_url = "srt://user:secret@example.test:9000?token=hidden";
        let stream = ResolvedRemoteStream {
            source: RemoteStreamSource::Addon(RemoteStreamCommand {
                program: "producer".to_owned(),
                args: Vec::new(),
                working_directory: PathBuf::from("streams"),
                environment: BTreeMap::from([(
                    "ERSATZRS_REMOTE_STREAM_URL".to_owned(),
                    operation_url.to_owned(),
                )]),
            }),
            liveness: RemoteStreamLiveness::Finite,
            is_live: false,
            configured_duration: None,
            fallback_query: None,
            uses_addon: true,
        };
        assert_eq!(
            redact_remote_stream_output(operation_url, &stream),
            "[redacted]"
        );

        let file_secret_stream = ResolvedRemoteStream {
            source: RemoteStreamSource::Addon(RemoteStreamCommand {
                program: "producer".to_owned(),
                args: Vec::new(),
                working_directory: PathBuf::from("streams"),
                environment: BTreeMap::from([(
                    "ERSATZRS_ADDON_SECRET_FILE_TOKEN".to_owned(),
                    "/run/secrets/token".to_owned(),
                )]),
            }),
            liveness: RemoteStreamLiveness::Finite,
            is_live: false,
            configured_duration: None,
            fallback_query: None,
            uses_addon: true,
        };
        assert_eq!(
            redact_remote_stream_output("raw secret contents", &file_secret_stream),
            FILE_SECRET_DIAGNOSTIC
        );
    }

    #[test]
    fn parses_and_redacts_structured_producer_diagnostics() {
        let stderr = "provider details\n{\"code\":\"provider-rejected\",\"message\":\"token secret-value rejected\"}\n";
        let diagnostic = producer_operation_diagnostic(stderr, &["secret-value".to_owned()], false);
        assert_eq!(diagnostic.code, "provider-rejected");
        assert!(diagnostic.message.contains("[redacted]"));
        assert!(!diagnostic.message.contains("secret-value"));

        let suppressed = producer_operation_diagnostic(stderr, &[], true);
        assert_eq!(suppressed.code, "provider-rejected");
        assert_eq!(suppressed.message, FILE_SECRET_DIAGNOSTIC);
    }

    #[test]
    fn legacy_producer_stderr_uses_generic_operation_code() {
        let diagnostic = producer_operation_diagnostic("legacy failure", &[], false);
        assert_eq!(diagnostic.code, OPERATION_CODE_FAILED);
        assert_eq!(diagnostic.message, "legacy failure");
    }

    #[test]
    fn legacy_producer_diagnostic_preserves_the_final_bounded_stderr_bytes() {
        let stderr = format!("old diagnostic {} final failure �", "x".repeat(2_048));
        let diagnostic = producer_operation_diagnostic(&stderr, &[], false);

        assert_eq!(diagnostic.code, OPERATION_CODE_FAILED);
        assert!(diagnostic.message.len() <= MAX_OPERATION_ERROR_MESSAGE_BYTES);
        assert!(diagnostic.message.ends_with(" final failure �"));
        assert!(!diagnostic.message.contains("old diagnostic"));
        assert!(diagnostic
            .message
            .is_char_boundary(diagnostic.message.len()));
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
