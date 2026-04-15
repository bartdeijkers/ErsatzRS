//! `FFmpeg` runtime version detection.
//!
//! `NasTv` requires `FFmpeg` >= 8.1 at runtime for the `drawvg` filter used by the
//! graphics engine. This module provides a startup check that parses
//! the version string returned by `ffmpeg -version` and exits early with a clear
//! error message if the requirement is not met.

/// Minimum required `FFmpeg` version.
const REQUIRED_MAJOR: u32 = 8;
const REQUIRED_MINOR: u32 = 1;

/// Error returned when `FFmpeg` is missing or too old.
#[derive(thiserror::Error, Debug)]
pub enum FfmpegCheckError {
    /// `ffmpeg` executable was not found or could not be spawned.
    #[error("FFmpeg not found: {0}. Please install FFmpeg >= {REQUIRED_MAJOR}.{REQUIRED_MINOR}.")]
    NotFound(String),
    /// The version string could not be parsed.
    #[error("Could not parse FFmpeg version from output: {0:?}")]
    ParseError(String),
    /// `FFmpeg` was found but is older than the required version.
    #[error("FFmpeg {found} is too old. NasTv requires FFmpeg >= {required_major}.{required_minor} (for the `drawvg` filter).")]
    TooOld {
        found: FfmpegVersion,
        required_major: u32,
        required_minor: u32,
    },
}

/// Parsed `FFmpeg` version.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FfmpegVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl FfmpegVersion {
    /// Returns `true` if this version satisfies the `>= major.minor` requirement.
    pub fn satisfies(&self, req_major: u32, req_minor: u32) -> bool {
        (self.major, self.minor) >= (req_major, req_minor)
    }
}

impl std::fmt::Display for FfmpegVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Parse the version triple from the first line of `ffmpeg -version` output.
///
/// Accepted formats (non-exhaustive):
/// - `ffmpeg version 8.1.1 Copyright ...`
/// - `ffmpeg version n8.1-0-gabcdef1234 Copyright ...`
/// - `ffmpeg version 8.1 Copyright ...`
pub fn parse_version(first_line: &str) -> Result<FfmpegVersion, FfmpegCheckError> {
    // The first line starts with "ffmpeg version <token> ..."
    // <token> may be a plain semver ("8.1.1"), a tag ("n8.1.1"), or a
    // describe string ("n8.1-0-gabcdef"). We strip a leading 'n' and take
    // the part before any '-' to get the dotted numeric prefix.
    let token = first_line
        .split_whitespace()
        .nth(2)
        .ok_or_else(|| FfmpegCheckError::ParseError(first_line.to_owned()))?;

    // Strip leading 'n' (release tag prefix used by some builds).
    let token = token.strip_prefix('n').unwrap_or(token);

    // Take the portion before the first '-' (strips git describe suffix).
    let numeric = token.split('-').next().unwrap_or(token);

    let mut parts = numeric.splitn(3, '.');
    let major = parse_component(parts.next(), first_line)?;
    let minor = parse_component(parts.next(), first_line)?;
    // Patch is optional (e.g. "8.1" with no patch component).
    let patch = parts
        .next()
        .map(|s| {
            s.parse::<u32>()
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
    part.and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| FfmpegCheckError::ParseError(line.to_owned()))
}

/// Spawn `ffmpeg -version`, parse the version, and return it.
///
/// Returns `Err` if the binary is not found, the output cannot be parsed, or
/// the version does not meet the `>= 8.1` requirement.
pub async fn check_ffmpeg_version() -> Result<FfmpegVersion, FfmpegCheckError> {
    let output = tokio::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .await
        .map_err(|e| FfmpegCheckError::NotFound(e.to_string()))?;

    if !output.status.success() {
        return Err(FfmpegCheckError::NotFound(format!(
            "ffmpeg exited with status {}",
            output.status
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| FfmpegCheckError::ParseError(e.to_string()))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_semver() {
        let v = parse_version("ffmpeg version 8.1.1 Copyright (C) 2000-2024 the FFmpeg developers")
            .unwrap();
        assert_eq!(
            v,
            FfmpegVersion {
                major: 8,
                minor: 1,
                patch: 1
            }
        );
    }

    #[test]
    fn parses_tag_prefix() {
        let v = parse_version(
            "ffmpeg version n8.1-0-gabcdef1234 Copyright (C) 2000-2024 the FFmpeg developers",
        )
        .unwrap();
        assert_eq!(
            v,
            FfmpegVersion {
                major: 8,
                minor: 1,
                patch: 0
            }
        );
    }

    #[test]
    fn parses_no_patch() {
        let v = parse_version("ffmpeg version 8.1 Copyright (C) 2000-2024 the FFmpeg developers")
            .unwrap();
        assert_eq!(
            v,
            FfmpegVersion {
                major: 8,
                minor: 1,
                patch: 0
            }
        );
    }

    #[test]
    fn parses_higher_version() {
        let v = parse_version("ffmpeg version 9.0.2 Copyright (C) 2000-2025 the FFmpeg developers")
            .unwrap();
        assert_eq!(
            v,
            FfmpegVersion {
                major: 9,
                minor: 0,
                patch: 2
            }
        );
    }

    #[test]
    fn satisfies_exact_required() {
        let v = FfmpegVersion {
            major: 8,
            minor: 1,
            patch: 0,
        };
        assert!(v.satisfies(8, 1));
    }

    #[test]
    fn satisfies_newer_minor() {
        let v = FfmpegVersion {
            major: 8,
            minor: 2,
            patch: 0,
        };
        assert!(v.satisfies(8, 1));
    }

    #[test]
    fn satisfies_newer_major() {
        let v = FfmpegVersion {
            major: 9,
            minor: 0,
            patch: 0,
        };
        assert!(v.satisfies(8, 1));
    }

    #[test]
    fn rejects_too_old() {
        let v = FfmpegVersion {
            major: 8,
            minor: 0,
            patch: 5,
        };
        assert!(!v.satisfies(8, 1));
    }

    #[test]
    fn rejects_much_older() {
        let v = FfmpegVersion {
            major: 6,
            minor: 1,
            patch: 0,
        };
        assert!(!v.satisfies(8, 1));
    }

    #[test]
    fn error_on_missing_version_token() {
        // Only 2 tokens — nth(2) returns None, covering the token-not-found path.
        assert!(parse_version("ffmpeg version").is_err());
    }

    #[test]
    fn error_on_non_numeric_version_token() {
        // 3 tokens but the third is not a valid version number.
        assert!(parse_version("ffmpeg version abc").is_err());
    }

    #[test]
    fn error_on_empty_string() {
        assert!(parse_version("").is_err());
    }
}
