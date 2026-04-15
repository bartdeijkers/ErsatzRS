//! `FFmpeg` runtime version detection.
//!
//! `NasTv` requires `FFmpeg` >= 8.1 at runtime for the `drawvg` filter used by the
//! graphics engine. This module provides a synchronous startup check that parses
//! the version string returned by `ffmpeg -version` and exits early with a clear
//! error message if the requirement is not met.

use std::process::Command;

/// Minimum required `FFmpeg` version.
const REQUIRED_MAJOR: u32 = 8;
const REQUIRED_MINOR: u32 = 1;

/// Error returned when `FFmpeg` is missing or too old.
#[derive(Debug)]
pub enum FfmpegCheckError {
    /// `ffmpeg` executable was not found or could not be spawned.
    NotFound(String),
    /// The version string could not be parsed.
    ParseError(String),
    /// `FFmpeg` was found but is older than the required version.
    TooOld {
        found: (u32, u32, u32),
        required: (u32, u32),
    },
}

impl std::fmt::Display for FfmpegCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FfmpegCheckError::NotFound(msg) => {
                write!(f, "FFmpeg not found: {msg}. Please install FFmpeg >= {REQUIRED_MAJOR}.{REQUIRED_MINOR}.")
            }
            FfmpegCheckError::ParseError(line) => {
                write!(f, "Could not parse FFmpeg version from output: {line:?}")
            }
            FfmpegCheckError::TooOld {
                found: (maj, min, patch),
                required: (req_maj, req_min),
            } => {
                write!(
                    f,
                    "FFmpeg {maj}.{min}.{patch} is too old. NasTv requires FFmpeg >= {req_maj}.{req_min} (for the `drawvg` filter)."
                )
            }
        }
    }
}

/// Parsed `FFmpeg` version.
#[derive(Debug, PartialEq, Eq)]
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
pub fn check_ffmpeg_version() -> Result<FfmpegVersion, FfmpegCheckError> {
    let output = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map_err(|e| FfmpegCheckError::NotFound(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout
        .lines()
        .next()
        .ok_or_else(|| FfmpegCheckError::ParseError(String::new()))?;

    let version = parse_version(first_line)?;

    if !version.satisfies(REQUIRED_MAJOR, REQUIRED_MINOR) {
        return Err(FfmpegCheckError::TooOld {
            found: (version.major, version.minor, version.patch),
            required: (REQUIRED_MAJOR, REQUIRED_MINOR),
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
    fn error_on_malformed_line() {
        assert!(parse_version("not a valid ffmpeg output").is_err());
    }

    #[test]
    fn error_on_empty_string() {
        assert!(parse_version("").is_err());
    }
}
