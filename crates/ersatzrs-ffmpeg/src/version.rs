/// Minimum required `FFmpeg` version.
const REQUIRED_MAJOR: u32 = 8;
const REQUIRED_MINOR: u32 = 1;

#[derive(thiserror::Error, Debug)]
pub enum FfmpegCheckError {
    #[error("FFmpeg not found: {0}. Please install FFmpeg >= {REQUIRED_MAJOR}.{REQUIRED_MINOR}.")]
    NotFound(String),
    #[error("Could not parse FFmpeg version from output: {0:?}")]
    ParseError(String),
    #[error("FFmpeg {found} is too old. ErsatzRS requires FFmpeg >= {required_major}.{required_minor} for the drawvg filter.")]
    TooOld {
        found: FfmpegVersion,
        required_major: u32,
        required_minor: u32,
    },
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FfmpegVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl FfmpegVersion {
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

/// Parse the version triple from the first line of `ffmpeg -version` output.
///
/// # Errors
///
/// Returns [`FfmpegCheckError::ParseError`] when the line does not contain a
/// parseable major/minor version.
pub fn parse_version(first_line: &str) -> Result<FfmpegVersion, FfmpegCheckError> {
    let token = first_line
        .split_whitespace()
        .nth(2)
        .ok_or_else(|| FfmpegCheckError::ParseError(first_line.to_owned()))?;
    let token = token.strip_prefix('n').unwrap_or(token);
    let numeric = token.split('-').next().unwrap_or(token);

    let mut parts = numeric.splitn(3, '.');
    let major = parse_component(parts.next(), first_line)?;
    let minor = parse_component(parts.next(), first_line)?;
    let patch = parts
        .next()
        .map(|part| {
            part.parse::<u32>()
                .map_err(|_| FfmpegCheckError::ParseError(first_line.to_owned()))
        })
        .transpose()?
        .unwrap_or(0);

    Ok(FfmpegVersion {
        major,
        minor,
        patch,
    })
}

fn parse_component(part: Option<&str>, line: &str) -> Result<u32, FfmpegCheckError> {
    part.and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| FfmpegCheckError::ParseError(line.to_owned()))
}

/// Spawn `ffmpeg -version`, parse the version, and enforce the runtime floor.
///
/// # Errors
///
/// Returns an error if `ffmpeg` cannot be spawned, prints unparsable output, or
/// reports a version older than 8.1.
pub async fn check_ffmpeg_version() -> Result<FfmpegVersion, FfmpegCheckError> {
    let output = tokio::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .await
        .map_err(|error| FfmpegCheckError::NotFound(error.to_string()))?;

    if !output.status.success() {
        return Err(FfmpegCheckError::NotFound(format!(
            "ffmpeg exited with status {}",
            output.status
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| FfmpegCheckError::ParseError(error.to_string()))?;
    let first_line = stdout
        .lines()
        .next()
        .ok_or_else(|| FfmpegCheckError::ParseError(String::new()))?;

    let version = parse_version(first_line)?;
    if !version.satisfies(REQUIRED_MAJOR, REQUIRED_MINOR) {
        return Err(FfmpegCheckError::TooOld {
            found: version,
            required_major: REQUIRED_MAJOR,
            required_minor: REQUIRED_MINOR,
        });
    }

    Ok(version)
}
