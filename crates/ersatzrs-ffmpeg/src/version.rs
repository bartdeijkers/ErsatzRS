use std::collections::BTreeSet;
use std::process::ExitStatus;

/// Minimum required `FFmpeg` and `ffprobe` major version.
pub const REQUIRED_MAJOR: u32 = 8;
/// Minimum required `FFmpeg` and `ffprobe` minor version.
pub const REQUIRED_MINOR: u32 = 1;

pub(crate) const DEFAULT_FFMPEG_PATH: &str = "ffmpeg";
pub(crate) const DEFAULT_FFPROBE_PATH: &str = "ffprobe";
pub(crate) const FFMPEG_PATH_ENV_VARS: &[&str] = &[
    "ErsatzRS_FFMPEG__PATH",
    "ERSATZRS_FFMPEG__PATH",
    "FFMPEG_PATH",
];
pub(crate) const FFPROBE_PATH_ENV_VARS: &[&str] = &[
    "ErsatzRS_FFPROBE__PATH",
    "ERSATZRS_FFPROBE__PATH",
    "FFPROBE_PATH",
    "ERSATZRS_FFPROBE",
];

const REQUIRED_FILTERS: &[&str] = &["drawvg", "format", "overlay", "scale", "subtitles"];
const REQUIRED_ENCODERS: &[&str] = &["aac", "ac3", "libsvtav1", "libx264", "libx265"];
const REQUIRED_DECODERS: &[&str] = &["av1", "h264", "hevc"];
const REQUIRED_CONFIGURATION_FLAGS: &[&str] = &[
    "--enable-cairo",
    "--enable-libfontconfig",
    "--enable-libass",
    "--enable-libfreetype",
    "--enable-libharfbuzz",
];

/// Errors returned while checking the local `FFmpeg` toolchain.
#[derive(thiserror::Error, Debug)]
pub enum FfmpegCheckError {
    /// A required binary could not be spawned.
    #[error("{tool} not found at {path}: {error}. Please install {tool} >= {required_major}.{required_minor}.")]
    ToolNotFound {
        /// Binary name, such as `ffmpeg` or `ffprobe`.
        tool: &'static str,
        /// Path that was attempted.
        path: String,
        /// Minimum required major version.
        required_major: u32,
        /// Minimum required minor version.
        required_minor: u32,
        /// Spawn error text.
        error: String,
    },
    /// A required binary exited unsuccessfully.
    #[error("{tool} at {path} exited with status {status}: {stderr}")]
    ToolFailed {
        /// Binary name, such as `ffmpeg` or `ffprobe`.
        tool: &'static str,
        /// Path that was attempted.
        path: String,
        /// Process exit status.
        status: String,
        /// Captured stderr.
        stderr: String,
    },
    /// The version banner did not contain a parseable version.
    #[error("could not parse {tool} version from output: {output:?}")]
    ParseError {
        /// Binary name, such as `ffmpeg` or `ffprobe`.
        tool: &'static str,
        /// Output that could not be parsed.
        output: String,
    },
    /// A binary is older than the required runtime floor.
    #[error("{tool} {found} is too old. ErsatzRS requires {tool} >= {required_major}.{required_minor} for FFmpeg 8.1 drawvg support.")]
    TooOld {
        /// Binary name, such as `ffmpeg` or `ffprobe`.
        tool: &'static str,
        /// Version reported by the binary.
        found: FfmpegVersion,
        /// Minimum required major version.
        required_major: u32,
        /// Minimum required minor version.
        required_minor: u32,
    },
    /// One or more required `FFmpeg` capabilities were absent.
    #[error("FFmpeg runtime is missing required capabilities: {0:?}")]
    RuntimeIssues(Vec<FfmpegRuntimeIssue>),
}

/// Parsed semantic version from an FFmpeg-family version banner.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FfmpegVersion {
    /// Major version.
    pub major: u32,
    /// Minor version.
    pub minor: u32,
    /// Patch version, or zero when the banner omits it.
    pub patch: u32,
}

impl FfmpegVersion {
    /// Returns whether this version satisfies a major/minor runtime floor.
    #[must_use]
    pub const fn satisfies(&self, req_major: u32, req_minor: u32) -> bool {
        self.major > req_major || self.major == req_major && self.minor >= req_minor
    }
}

impl std::fmt::Display for FfmpegVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Runtime check inputs, including binary paths and required capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegRuntimeCheckConfig {
    /// Path to the `ffmpeg` binary.
    pub ffmpeg_path: String,
    /// Path to the `ffprobe` binary.
    pub ffprobe_path: String,
    /// Required major version.
    pub required_major: u32,
    /// Required minor version.
    pub required_minor: u32,
    /// Filters that must be present in `ffmpeg -filters`.
    pub required_filters: Vec<String>,
    /// Encoders that must be present in `ffmpeg -encoders`.
    pub required_encoders: Vec<String>,
    /// Decoders that must be present in `ffmpeg -decoders`.
    pub required_decoders: Vec<String>,
    /// Configuration flags that must appear in `ffmpeg -version`.
    pub required_configuration_flags: Vec<String>,
    /// Hardware accelerators that must be present.
    pub required_hwaccels: Vec<String>,
    /// Hardware accelerators to report as warnings when absent.
    pub optional_hwaccels: Vec<String>,
}

impl Default for FfmpegRuntimeCheckConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: ffmpeg_path_from_env(),
            ffprobe_path: ffprobe_path_from_env(),
            required_major: REQUIRED_MAJOR,
            required_minor: REQUIRED_MINOR,
            required_filters: to_owned_vec(REQUIRED_FILTERS),
            required_encoders: to_owned_vec(REQUIRED_ENCODERS),
            required_decoders: to_owned_vec(REQUIRED_DECODERS),
            required_configuration_flags: to_owned_vec(REQUIRED_CONFIGURATION_FLAGS),
            required_hwaccels: Vec::new(),
            optional_hwaccels: Vec::new(),
        }
    }
}

impl FfmpegRuntimeCheckConfig {
    /// Create the default check config from process environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        Self::default()
    }

    /// Create a check config with explicit, already-resolved tool paths.
    ///
    /// Used by startup after the [`crate::locator`] has resolved the `ffmpeg`
    /// and `ffprobe` paths from DB config / next-to-executable / `PATH`, so the
    /// runtime report reflects the same paths that spawn sites will use.
    #[must_use]
    pub fn with_paths(ffmpeg_path: impl Into<String>, ffprobe_path: impl Into<String>) -> Self {
        Self {
            ffmpeg_path: ffmpeg_path.into(),
            ffprobe_path: ffprobe_path.into(),
            ..Self::default()
        }
    }
}

/// Version details for one FFmpeg-family binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegToolVersion {
    /// Binary name, such as `ffmpeg` or `ffprobe`.
    pub tool: &'static str,
    /// Path that was executed.
    pub path: String,
    /// Parsed version.
    pub version: FfmpegVersion,
    /// Configure flags parsed from the version banner.
    pub configuration_flags: BTreeSet<String>,
}

/// Capability names reported by the local `ffmpeg` binary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FfmpegCapabilityReport {
    /// Filter names from `ffmpeg -filters`.
    pub filters: BTreeSet<String>,
    /// Encoder names from `ffmpeg -encoders`.
    pub encoders: BTreeSet<String>,
    /// Decoder names from `ffmpeg -decoders`.
    pub decoders: BTreeSet<String>,
    /// Hardware accelerator names from `ffmpeg -hwaccels`.
    pub hwaccels: BTreeSet<String>,
    /// Configure flags parsed from `ffmpeg -version`.
    pub configuration_flags: BTreeSet<String>,
}

impl FfmpegCapabilityReport {
    /// Returns whether a named filter is present.
    #[must_use]
    pub fn has_filter(&self, name: &str) -> bool {
        self.filters.contains(name)
    }

    /// Returns whether a named encoder is present.
    #[must_use]
    pub fn has_encoder(&self, name: &str) -> bool {
        self.encoders.contains(name)
    }

    /// Returns whether a named decoder is present.
    #[must_use]
    pub fn has_decoder(&self, name: &str) -> bool {
        self.decoders.contains(name)
    }

    /// Returns whether a named hardware accelerator is present.
    #[must_use]
    pub fn has_hwaccel(&self, name: &str) -> bool {
        self.hwaccels.contains(name)
    }

    /// Returns whether a configure flag is present.
    #[must_use]
    pub fn has_configuration_flag(&self, flag: &str) -> bool {
        self.configuration_flags.contains(flag)
    }
}

/// Non-fatal or fatal capability issue discovered after both tools are present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FfmpegRuntimeIssue {
    /// Required filter is missing.
    RequiredFilter { name: String },
    /// Required encoder is missing.
    RequiredEncoder { name: String },
    /// Required decoder is missing.
    RequiredDecoder { name: String },
    /// Required configure flag is missing.
    RequiredConfigurationFlag { flag: String },
    /// Required hardware accelerator is missing.
    RequiredHwaccel { name: String },
    /// Optional hardware accelerator is missing.
    OptionalHwaccel { name: String },
}

impl FfmpegRuntimeIssue {
    /// Returns true when the issue should fail startup.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        !matches!(self, Self::OptionalHwaccel { .. })
    }
}

/// Full `FFmpeg` runtime report used by startup, health, and the admin UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegRuntimeReport {
    /// `ffmpeg` version details.
    pub ffmpeg: FfmpegToolVersion,
    /// `ffprobe` version details.
    pub ffprobe: FfmpegToolVersion,
    /// Capabilities reported by `ffmpeg`.
    pub capabilities: FfmpegCapabilityReport,
    /// Capability issues found during the runtime check.
    pub issues: Vec<FfmpegRuntimeIssue>,
}

impl FfmpegRuntimeReport {
    /// Returns true when the report contains any fatal issue.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(FfmpegRuntimeIssue::is_error)
    }

    /// Count fatal and warning issues.
    #[must_use]
    pub fn issue_counts(&self) -> (u32, u32) {
        let errors = self
            .issues
            .iter()
            .filter(|issue| issue.is_error())
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        let warnings = self
            .issues
            .iter()
            .filter(|issue| !issue.is_error())
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        (errors, warnings)
    }

    /// Minimal report used by tests that do not need to spawn `FFmpeg`.
    #[must_use]
    pub fn for_tests() -> Self {
        let version = FfmpegVersion {
            major: REQUIRED_MAJOR,
            minor: REQUIRED_MINOR,
            patch: 0,
        };
        let flags = BTreeSet::from_iter(to_owned_vec(REQUIRED_CONFIGURATION_FLAGS));
        let capabilities = FfmpegCapabilityReport {
            filters: BTreeSet::from_iter(to_owned_vec(REQUIRED_FILTERS)),
            encoders: BTreeSet::from_iter(to_owned_vec(REQUIRED_ENCODERS)),
            decoders: BTreeSet::from_iter(to_owned_vec(REQUIRED_DECODERS)),
            hwaccels: BTreeSet::new(),
            configuration_flags: flags.clone(),
        };
        Self {
            ffmpeg: FfmpegToolVersion {
                tool: "ffmpeg",
                path: DEFAULT_FFMPEG_PATH.to_owned(),
                version: version.clone(),
                configuration_flags: flags.clone(),
            },
            ffprobe: FfmpegToolVersion {
                tool: "ffprobe",
                path: DEFAULT_FFPROBE_PATH.to_owned(),
                version,
                configuration_flags: flags,
            },
            capabilities,
            issues: Vec::new(),
        }
    }
}

/// Resolve the `ffmpeg` path from supported environment variables.
#[must_use]
pub fn ffmpeg_path_from_env() -> String {
    path_from_env(FFMPEG_PATH_ENV_VARS, DEFAULT_FFMPEG_PATH)
}

/// Resolve the `ffprobe` path from supported environment variables.
#[must_use]
pub fn ffprobe_path_from_env() -> String {
    path_from_env(FFPROBE_PATH_ENV_VARS, DEFAULT_FFPROBE_PATH)
}

fn path_from_env(keys: &[&str], default: &str) -> String {
    crate::locator::first_non_empty_env(keys).unwrap_or_else(|| default.to_owned())
}

/// Parse the version triple from the first line of `ffmpeg -version` or
/// `ffprobe -version` output.
///
/// # Errors
///
/// Returns [`FfmpegCheckError::ParseError`] when the line does not contain a
/// parseable major/minor version.
pub fn parse_version(first_line: &str) -> Result<FfmpegVersion, FfmpegCheckError> {
    parse_version_for_tool("ffmpeg", first_line)
}

fn parse_version_for_tool(
    tool: &'static str,
    first_line: &str,
) -> Result<FfmpegVersion, FfmpegCheckError> {
    let token =
        first_line
            .split_whitespace()
            .nth(2)
            .ok_or_else(|| FfmpegCheckError::ParseError {
                tool,
                output: first_line.to_owned(),
            })?;
    let token = token.strip_prefix('n').unwrap_or(token);
    let numeric = token.split('-').next().unwrap_or(token);

    let mut parts = numeric.splitn(3, '.');
    let major = parse_component(tool, parts.next(), first_line)?;
    let minor = parse_component(tool, parts.next(), first_line)?;
    let patch = parts
        .next()
        .map(|part| {
            part.parse::<u32>()
                .map_err(|_| FfmpegCheckError::ParseError {
                    tool,
                    output: first_line.to_owned(),
                })
        })
        .transpose()?
        .unwrap_or(0);

    Ok(FfmpegVersion {
        major,
        minor,
        patch,
    })
}

fn parse_component(
    tool: &'static str,
    part: Option<&str>,
    line: &str,
) -> Result<u32, FfmpegCheckError> {
    part.and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| FfmpegCheckError::ParseError {
            tool,
            output: line.to_owned(),
        })
}

/// Parse configure flags from an FFmpeg-family version banner.
#[must_use]
pub fn parse_configuration_flags(output: &str) -> BTreeSet<String> {
    let mut flags: BTreeSet<String> = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("configuration:"))
        .map(|line| {
            line.split_whitespace()
                .filter(|token| token.starts_with("--"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();

    if flags.contains("--enable-fontconfig") {
        flags.insert("--enable-libfontconfig".to_owned());
    }
    if flags.contains("--enable-libfontconfig") {
        flags.insert("--enable-fontconfig".to_owned());
    }

    flags
}

/// Parse names from `ffmpeg -filters`, `ffmpeg -encoders`, or
/// `ffmpeg -decoders` output.
#[must_use]
pub fn parse_capability_names(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let flags = parts.next()?;
            let name = parts.next()?;
            let is_capability_line = flags.len() >= 2
                && flags
                    .chars()
                    .all(|ch| ch == '.' || ch.is_ascii_alphabetic());
            (is_capability_line && name != "=").then(|| name.to_owned())
        })
        .collect()
}

/// Parse names from `ffmpeg -hwaccels` output.
#[must_use]
pub fn parse_hwaccels(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("Hardware acceleration methods"))
        .map(str::to_owned)
        .collect()
}

/// Spawn `ffmpeg -version`, parse the version, and enforce the runtime floor.
///
/// # Errors
///
/// Returns an error if `ffmpeg` cannot be spawned, prints unparsable output, or
/// reports a version older than 8.1.
pub async fn check_ffmpeg_version() -> Result<FfmpegVersion, FfmpegCheckError> {
    let config = FfmpegRuntimeCheckConfig::from_env();
    Ok(check_tool_version(
        "ffmpeg",
        &config.ffmpeg_path,
        config.required_major,
        config.required_minor,
    )
    .await?
    .version)
}

/// Check the local `FFmpeg` runtime using environment-derived paths.
///
/// # Errors
///
/// Returns an error when `ffmpeg` or `ffprobe` is missing, too old, exits
/// unsuccessfully, prints unparsable version output, or lacks required
/// capabilities.
pub async fn check_ffmpeg_runtime() -> Result<FfmpegRuntimeReport, FfmpegCheckError> {
    check_ffmpeg_runtime_with_config(&FfmpegRuntimeCheckConfig::from_env()).await
}

/// Check the local `FFmpeg` runtime using an explicit config.
///
/// # Errors
///
/// Returns an error when `ffmpeg` or `ffprobe` is missing, too old, exits
/// unsuccessfully, prints unparsable version output, or lacks required
/// capabilities.
pub async fn check_ffmpeg_runtime_with_config(
    config: &FfmpegRuntimeCheckConfig,
) -> Result<FfmpegRuntimeReport, FfmpegCheckError> {
    let ffmpeg = check_tool_version(
        "ffmpeg",
        &config.ffmpeg_path,
        config.required_major,
        config.required_minor,
    )
    .await?;
    let ffprobe = check_tool_version(
        "ffprobe",
        &config.ffprobe_path,
        config.required_major,
        config.required_minor,
    )
    .await?;
    check_ffprobe_json(&config.ffprobe_path).await?;

    let mut capabilities = collect_ffmpeg_capabilities(&config.ffmpeg_path).await?;
    capabilities
        .configuration_flags
        .clone_from(&ffmpeg.configuration_flags);

    let issues = collect_runtime_issues(config, &capabilities);
    let report = FfmpegRuntimeReport {
        ffmpeg,
        ffprobe,
        capabilities,
        issues,
    };

    if report.has_errors() {
        return Err(FfmpegCheckError::RuntimeIssues(
            report
                .issues
                .iter()
                .filter(|issue| issue.is_error())
                .cloned()
                .collect(),
        ));
    }

    Ok(report)
}

async fn check_tool_version(
    tool: &'static str,
    path: &str,
    required_major: u32,
    required_minor: u32,
) -> Result<FfmpegToolVersion, FfmpegCheckError> {
    let output = run_tool(tool, path, &["-version"]).await?;
    let stdout = String::from_utf8(output).map_err(|error| FfmpegCheckError::ParseError {
        tool,
        output: error.to_string(),
    })?;
    let first_line = stdout
        .lines()
        .next()
        .ok_or_else(|| FfmpegCheckError::ParseError {
            tool,
            output: String::new(),
        })?;

    let version = parse_version_for_tool(tool, first_line)?;
    if !version.satisfies(required_major, required_minor) {
        return Err(FfmpegCheckError::TooOld {
            tool,
            found: version,
            required_major,
            required_minor,
        });
    }

    Ok(FfmpegToolVersion {
        tool,
        path: path.to_owned(),
        version,
        configuration_flags: parse_configuration_flags(&stdout),
    })
}

async fn collect_ffmpeg_capabilities(
    ffmpeg_path: &str,
) -> Result<FfmpegCapabilityReport, FfmpegCheckError> {
    let filters =
        String::from_utf8(run_tool("ffmpeg", ffmpeg_path, &["-hide_banner", "-filters"]).await?)
            .map_err(|error| FfmpegCheckError::ParseError {
                tool: "ffmpeg",
                output: error.to_string(),
            })?;
    let encoders =
        String::from_utf8(run_tool("ffmpeg", ffmpeg_path, &["-hide_banner", "-encoders"]).await?)
            .map_err(|error| FfmpegCheckError::ParseError {
            tool: "ffmpeg",
            output: error.to_string(),
        })?;
    let decoders =
        String::from_utf8(run_tool("ffmpeg", ffmpeg_path, &["-hide_banner", "-decoders"]).await?)
            .map_err(|error| FfmpegCheckError::ParseError {
            tool: "ffmpeg",
            output: error.to_string(),
        })?;
    let hwaccels =
        String::from_utf8(run_tool("ffmpeg", ffmpeg_path, &["-hide_banner", "-hwaccels"]).await?)
            .map_err(|error| FfmpegCheckError::ParseError {
            tool: "ffmpeg",
            output: error.to_string(),
        })?;

    Ok(FfmpegCapabilityReport {
        filters: parse_capability_names(&filters),
        encoders: parse_capability_names(&encoders),
        decoders: parse_capability_names(&decoders),
        hwaccels: parse_hwaccels(&hwaccels),
        configuration_flags: BTreeSet::new(),
    })
}

async fn check_ffprobe_json(ffprobe_path: &str) -> Result<(), FfmpegCheckError> {
    let output = run_tool(
        "ffprobe",
        ffprobe_path,
        &["-v", "error", "-of", "json", "-show_program_version"],
    )
    .await?;
    let stdout = String::from_utf8(output).map_err(|error| FfmpegCheckError::ParseError {
        tool: "ffprobe",
        output: error.to_string(),
    })?;

    if !stdout.contains("\"program_version\"") {
        return Err(FfmpegCheckError::ParseError {
            tool: "ffprobe",
            output: stdout,
        });
    }

    Ok(())
}

async fn run_tool(
    tool: &'static str,
    path: &str,
    args: &[&str],
) -> Result<Vec<u8>, FfmpegCheckError> {
    let output = tokio::process::Command::new(path)
        .args(args)
        .output()
        .await
        .map_err(|error| FfmpegCheckError::ToolNotFound {
            tool,
            path: path.to_owned(),
            required_major: REQUIRED_MAJOR,
            required_minor: REQUIRED_MINOR,
            error: error.to_string(),
        })?;

    if !output.status.success() {
        return Err(FfmpegCheckError::ToolFailed {
            tool,
            path: path.to_owned(),
            status: status_string(output.status),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(output.stdout)
}

fn status_string(status: ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| status.to_string(), |code| code.to_string())
}

fn collect_runtime_issues(
    config: &FfmpegRuntimeCheckConfig,
    capabilities: &FfmpegCapabilityReport,
) -> Vec<FfmpegRuntimeIssue> {
    let mut issues = Vec::new();

    for name in &config.required_filters {
        if !capabilities.has_filter(name) {
            issues.push(FfmpegRuntimeIssue::RequiredFilter { name: name.clone() });
        }
    }
    for name in &config.required_encoders {
        if !capabilities.has_encoder(name) {
            issues.push(FfmpegRuntimeIssue::RequiredEncoder { name: name.clone() });
        }
    }
    for name in &config.required_decoders {
        if !capabilities.has_decoder(name) {
            issues.push(FfmpegRuntimeIssue::RequiredDecoder { name: name.clone() });
        }
    }
    for flag in &config.required_configuration_flags {
        if !capabilities.has_configuration_flag(flag) {
            issues.push(FfmpegRuntimeIssue::RequiredConfigurationFlag { flag: flag.clone() });
        }
    }
    for name in &config.required_hwaccels {
        if !capabilities.has_hwaccel(name) {
            issues.push(FfmpegRuntimeIssue::RequiredHwaccel { name: name.clone() });
        }
    }
    for name in &config.optional_hwaccels {
        if !capabilities.has_hwaccel(name) {
            issues.push(FfmpegRuntimeIssue::OptionalHwaccel { name: name.clone() });
        }
    }

    issues
}

fn to_owned_vec(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        collect_runtime_issues, parse_capability_names, parse_configuration_flags, parse_hwaccels,
        parse_version, FfmpegCapabilityReport, FfmpegRuntimeCheckConfig, FfmpegRuntimeIssue,
        FfmpegVersion,
    };
    use std::collections::BTreeSet;

    #[test]
    fn parses_release_version_without_patch() {
        assert_eq!(
            parse_version("ffmpeg version 8.1 Copyright").unwrap(),
            FfmpegVersion {
                major: 8,
                minor: 1,
                patch: 0
            }
        );
    }

    #[test]
    fn parses_n_prefixed_git_version_with_patch_and_suffix() {
        assert_eq!(
            parse_version("ffprobe version n8.1.2-12-gabcdef Copyright").unwrap(),
            FfmpegVersion {
                major: 8,
                minor: 1,
                patch: 2
            }
        );
    }

    #[test]
    fn parses_distro_suffixed_version() {
        assert_eq!(
            parse_version("ffmpeg version 8.1.1-0ubuntu1 Copyright").unwrap(),
            FfmpegVersion {
                major: 8,
                minor: 1,
                patch: 1
            }
        );
    }

    #[test]
    fn parses_configuration_flags() {
        let flags = parse_configuration_flags(
            "ffmpeg version 8.1\nconfiguration: --enable-gpl --enable-cairo --enable-libx264\n",
        );

        assert!(flags.contains("--enable-cairo"));
        assert!(flags.contains("--enable-libx264"));
        assert!(!flags.contains("configuration:"));
    }

    #[test]
    fn parses_filter_encoder_and_decoder_names() {
        let output = "
Filters:
  TSC drawvg            V->V       Draw vector graphics.
  ... overlay           VV->V      Overlay video.
Encoders:
 V....D libx264         H.264 encoder
 A..... aac             AAC encoder
";
        let names = parse_capability_names(output);

        assert!(names.contains("drawvg"));
        assert!(names.contains("overlay"));
        assert!(names.contains("libx264"));
        assert!(names.contains("aac"));
        assert!(!names.contains("Filters:"));
        assert!(!names.contains("="));
    }

    #[test]
    fn parses_hwaccel_names() {
        let output = "Hardware acceleration methods:\nvaapi\ncuda\nvulkan\n";
        let names = parse_hwaccels(output);

        assert_eq!(
            names,
            BTreeSet::from(["cuda".to_owned(), "vaapi".to_owned(), "vulkan".to_owned()])
        );
    }

    #[test]
    fn reports_missing_required_and_optional_capabilities() {
        let config = FfmpegRuntimeCheckConfig {
            required_filters: vec!["drawvg".to_owned()],
            required_encoders: vec!["libx264".to_owned()],
            required_decoders: vec!["h264".to_owned()],
            required_configuration_flags: vec!["--enable-cairo".to_owned()],
            required_hwaccels: vec!["vaapi".to_owned()],
            optional_hwaccels: vec!["cuda".to_owned()],
            ..FfmpegRuntimeCheckConfig::default()
        };
        let capabilities = FfmpegCapabilityReport {
            filters: BTreeSet::new(),
            encoders: BTreeSet::from(["libx264".to_owned()]),
            decoders: BTreeSet::from(["h264".to_owned()]),
            hwaccels: BTreeSet::new(),
            configuration_flags: BTreeSet::new(),
        };

        let issues = collect_runtime_issues(&config, &capabilities);

        assert_eq!(
            issues,
            vec![
                FfmpegRuntimeIssue::RequiredFilter {
                    name: "drawvg".to_owned()
                },
                FfmpegRuntimeIssue::RequiredConfigurationFlag {
                    flag: "--enable-cairo".to_owned()
                },
                FfmpegRuntimeIssue::RequiredHwaccel {
                    name: "vaapi".to_owned()
                },
                FfmpegRuntimeIssue::OptionalHwaccel {
                    name: "cuda".to_owned()
                }
            ]
        );
        assert!(issues[0].is_error());
        assert!(!issues[3].is_error());
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::{
        check_ffmpeg_runtime_with_config, FfmpegCheckError, FfmpegRuntimeCheckConfig,
        FfmpegRuntimeIssue, FfmpegVersion,
    };
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_reports_missing_ffmpeg() {
        let config = test_config("/definitely/missing/ffmpeg", "/definitely/missing/ffprobe");

        let error = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect_err("missing ffmpeg should fail startup check");

        assert!(matches!(
            error,
            FfmpegCheckError::ToolNotFound { tool: "ffmpeg", .. }
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_reports_missing_ffprobe() {
        let dir = temp_test_dir("missing-ffprobe");
        let ffmpeg = write_ffmpeg_script(&dir, "ffmpeg", "8.1", true, &[]);
        let config = test_config(ffmpeg.to_str().expect("utf-8 path"), "/missing/ffprobe");

        let error = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect_err("missing ffprobe should fail startup check");

        assert!(matches!(
            error,
            FfmpegCheckError::ToolNotFound {
                tool: "ffprobe",
                ..
            }
        ));
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_rejects_too_old_ffmpeg() {
        let dir = temp_test_dir("old-ffmpeg");
        let ffmpeg = write_ffmpeg_script(&dir, "ffmpeg", "8.0", true, &[]);
        let ffprobe = write_ffprobe_script(&dir, "ffprobe", "8.1");
        let config = test_config(
            ffmpeg.to_str().expect("utf-8 path"),
            ffprobe.to_str().expect("utf-8 path"),
        );

        let error = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect_err("old ffmpeg should fail startup check");

        assert!(matches!(
            error,
            FfmpegCheckError::TooOld {
                tool: "ffmpeg",
                found: FfmpegVersion {
                    major: 8,
                    minor: 0,
                    ..
                },
                ..
            }
        ));
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_rejects_too_old_ffprobe() {
        let dir = temp_test_dir("old-ffprobe");
        let ffmpeg = write_ffmpeg_script(&dir, "ffmpeg", "8.1", true, &[]);
        let ffprobe = write_ffprobe_script(&dir, "ffprobe", "8.0");
        let config = test_config(
            ffmpeg.to_str().expect("utf-8 path"),
            ffprobe.to_str().expect("utf-8 path"),
        );

        let error = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect_err("old ffprobe should fail startup check");

        assert!(matches!(
            error,
            FfmpegCheckError::TooOld {
                tool: "ffprobe",
                found: FfmpegVersion {
                    major: 8,
                    minor: 0,
                    ..
                },
                ..
            }
        ));
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_rejects_missing_required_drawvg() {
        let dir = temp_test_dir("missing-drawvg");
        let ffmpeg = write_ffmpeg_script(&dir, "ffmpeg", "8.1", false, &[]);
        let ffprobe = write_ffprobe_script(&dir, "ffprobe", "8.1");
        let config = test_config(
            ffmpeg.to_str().expect("utf-8 path"),
            ffprobe.to_str().expect("utf-8 path"),
        );

        let error = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect_err("missing drawvg should fail startup check");

        assert!(
            matches!(error, FfmpegCheckError::RuntimeIssues(issues) if issues == vec![
                FfmpegRuntimeIssue::RequiredFilter {
                    name: "drawvg".to_owned()
                }
            ])
        );
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_reports_optional_gpu_missing_without_failing() {
        let dir = temp_test_dir("optional-gpu");
        let ffmpeg = write_ffmpeg_script(&dir, "ffmpeg", "8.1", true, &["cuda"]);
        let ffprobe = write_ffprobe_script(&dir, "ffprobe", "8.1");
        let mut config = test_config(
            ffmpeg.to_str().expect("utf-8 path"),
            ffprobe.to_str().expect("utf-8 path"),
        );
        config.optional_hwaccels = vec!["cuda".to_owned(), "vaapi".to_owned()];

        let report = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect("optional GPU absence should not fail startup check");

        assert_eq!(
            report.issues,
            vec![FfmpegRuntimeIssue::OptionalHwaccel {
                name: "vaapi".to_owned()
            }]
        );
        assert!(!report.has_errors());
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    fn test_config(ffmpeg_path: &str, ffprobe_path: &str) -> FfmpegRuntimeCheckConfig {
        FfmpegRuntimeCheckConfig {
            ffmpeg_path: ffmpeg_path.to_owned(),
            ffprobe_path: ffprobe_path.to_owned(),
            required_filters: vec!["drawvg".to_owned()],
            required_encoders: Vec::new(),
            required_decoders: Vec::new(),
            required_configuration_flags: Vec::new(),
            required_hwaccels: Vec::new(),
            optional_hwaccels: Vec::new(),
            ..FfmpegRuntimeCheckConfig::default()
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ersatzrs-ffmpeg-{name}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn write_ffmpeg_script(
        dir: &Path,
        name: &str,
        version: &str,
        include_drawvg: bool,
        hwaccels: &[&str],
    ) -> PathBuf {
        let drawvg_line = if include_drawvg {
            "  TSC drawvg            V->V       Draw vector graphics.\n"
        } else {
            ""
        };
        let hwaccel_lines = hwaccels.join("\\n");
        let body = format!(
            r#"#!/bin/sh
case "$*" in
  "-version")
    cat <<'EOF'
ffmpeg version {version} Copyright
configuration: --enable-cairo
EOF
    ;;
  "-hide_banner -filters")
    cat <<'EOF'
Filters:
  T.. = Timeline support
{drawvg_line}  ... overlay           VV->V      Overlay video.
EOF
    ;;
  "-hide_banner -encoders")
    cat <<'EOF'
Encoders:
 V....D libx264         H.264 encoder
EOF
    ;;
  "-hide_banner -decoders")
    cat <<'EOF'
Decoders:
 V....D h264            H.264 decoder
EOF
    ;;
  "-hide_banner -hwaccels")
    printf 'Hardware acceleration methods:\n{hwaccel_lines}\n'
    ;;
  *)
    exit 2
    ;;
esac
"#
        );
        write_script(dir, name, &body)
    }

    fn write_ffprobe_script(dir: &Path, name: &str, version: &str) -> PathBuf {
        let body = format!(
            r#"#!/bin/sh
case "$*" in
  "-version")
    cat <<'EOF'
ffprobe version {version} Copyright
configuration: --enable-cairo
EOF
    ;;
  "-v error -of json -show_program_version")
    printf '{{"program_version":{{"version":"{version}"}}}}\n'
    ;;
  *)
    exit 2
    ;;
esac
"#
        );
        write_script(dir, name, &body)
    }

    fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).expect("write fake tool script");
        let mut permissions = fs::metadata(&path)
            .expect("read fake tool metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("make fake tool executable");
        path
    }
}
