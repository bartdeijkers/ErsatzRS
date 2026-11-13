//! Hints bound to the exact argv emitted by the named final-TS command steps.

use std::fmt;

use sha2::{Digest as _, Sha256};

use crate::PipelineStep;

/// Generated final MPEG-TS rate/period hints, not source or runtime authority.
///
/// Derivation requires the named muxrate, PCR-period, container and final-pipe
/// steps. Opaque constructors and scripts do not infer these facts from argv.
/// A host must discover the sole programme and its actual PCR PID from bounded
/// source PSI, reserve an unoccupied authored PID and validate the actual clock.
/// Public steps may be caller supplied, and this value may be transferred to
/// identical argv: the checked generated shape and seal establish consistency,
/// not unforgeable command provenance or general `FFmpeg` argument validity.
#[derive(Clone, PartialEq, Eq)]
pub struct MpegTsOutputPolicy {
    muxrate_bps: u64,
    pcr_period_ms: u32,
    stderr_progress: bool,
    arguments_digest: [u8; 32],
}

impl MpegTsOutputPolicy {
    /// Positive muxrate supplied by the named generated output step.
    #[must_use]
    pub const fn muxrate_bps(&self) -> u64 {
        self.muxrate_bps
    }

    /// Positive PCR period supplied by the named generated output step.
    #[must_use]
    pub const fn pcr_period_ms(&self) -> u32 {
        self.pcr_period_ms
    }

    /// The sealed generated command reports completion on stderr, not media stdout.
    #[must_use]
    pub const fn reports_stderr_progress(&self) -> bool {
        self.stderr_progress
    }

    pub(crate) fn from_generated(steps: &[PipelineStep], arguments: &[String]) -> Option<Self> {
        let rate = named_options(steps, "mpegts_muxrate")?;
        let period = named_options(steps, "mpegts_pcr_period")?;
        let container = named_options(steps, "mpegts")?;
        let pipe = named_options(steps, "pipe")?;
        if !pair(rate, "-muxrate")
            || !pair(period, "-pcr_period")
            || !matches_container(container)
            || pipe != ["pipe:1"]
            || arguments.last().map(String::as_str) != Some("pipe:1")
        {
            return None;
        }
        let muxrate_bps = u64::try_from(rate[1].parse::<i64>().ok()?).ok()?;
        let pcr_period_ms = period[1].parse::<u32>().ok()?;
        if muxrate_bps == 0
            || pcr_period_ms == 0
            || rate[1] != muxrate_bps.to_string()
            || period[1] != pcr_period_ms.to_string()
            || !unique_flags(steps, arguments)
            || !generated_output_scope(steps)
        {
            return None;
        }
        Some(Self {
            muxrate_bps,
            pcr_period_ms,
            stderr_progress: has_stderr_progress(steps, arguments),
            arguments_digest: digest(arguments)?,
        })
    }

    pub(crate) fn matches(&self, arguments: &[String]) -> bool {
        digest(arguments) == Some(self.arguments_digest)
    }
}

impl fmt::Debug for MpegTsOutputPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MpegTsOutputPolicy")
            .field("muxrate_bps", &self.muxrate_bps)
            .field("pcr_period_ms", &self.pcr_period_ms)
            .finish_non_exhaustive()
    }
}

fn named_options<'a>(steps: &'a [PipelineStep], required: &str) -> Option<&'a [String]> {
    let mut found = None;
    for step in steps {
        let PipelineStep::Generic {
            name,
            global_options,
            filter_options,
            output_options,
            ..
        } = step
        else {
            continue;
        };
        if *name != required {
            continue;
        }
        if found.is_some() || !global_options.is_empty() || !filter_options.is_empty() {
            return None;
        }
        found = Some(output_options.as_slice());
    }
    found
}

fn pair(options: &[String], flag: &str) -> bool {
    options.len() == 2 && options[0] == flag
}

fn matches_container(options: &[String]) -> bool {
    options == ["-f", "mpegts"]
        || options == ["-f", "mpegts", "-mpegts_flags", "+initial_discontinuity"]
}

fn unique_flags(steps: &[PipelineStep], arguments: &[String]) -> bool {
    for flag in ["-muxrate", "-pcr_period"] {
        if arguments
            .iter()
            .filter(|argument| argument.as_str() == flag)
            .count()
            != 1
        {
            return false;
        }
    }
    // Input -f options belong to inputs, not output steps. This deliberately
    // checks generated scopes, rather than parsing arbitrary FFmpeg argv.
    if steps
        .iter()
        .flat_map(PipelineStep::output_options)
        .filter(|argument| argument.as_str() == "-f")
        .count()
        != 1
    {
        return false;
    }
    arguments.iter().enumerate().all(|(index, argument)| {
        !argument.starts_with("pipe:")
            || index + 1 == arguments.len()
            || index > 0 && arguments[index - 1] == "-i"
            || argument == "pipe:2"
                && index > 0
                && arguments[index - 1] == "-progress"
                && has_stderr_progress(steps, arguments)
    })
}

fn has_stderr_progress(steps: &[PipelineStep], arguments: &[String]) -> bool {
    let count = steps
        .iter()
        .filter(|step| {
            matches!(step, PipelineStep::Generic {
            name: "progress_stderr", global_options, filter_options, output_options, ..
        } if global_options == &["-progress", "pipe:2"]
            && filter_options.is_empty() && output_options.is_empty())
        })
        .count();
    count == 1
        && arguments
            .iter()
            .filter(|arg| arg.as_str() == "-progress")
            .count()
            == 1
}

// Only the repository's finite generated option grammar is recognized. A
// positional output consumes earlier muxer settings, so even a non-pipe sink
// before the final pipe must disable these hints. Unknown options fail closed;
// this is not an arbitrary FFmpeg command-line parser.
fn generated_output_scope(steps: &[PipelineStep]) -> bool {
    steps.iter().all(|step| {
        let pipe = matches!(step, PipelineStep::Generic { name: "pipe", .. });
        (pipe || generated_options(step.output_options()))
            && generated_options(step.filter_options())
    })
}

fn generated_options(options: &[String]) -> bool {
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match option.as_str() {
            "-noautoscale" => {}
            "-map"
            | "-c"
            | "-codec:v"
            | "-codec:a"
            | "-codec:s"
            | "-codec:d"
            | "-flags"
            | "-bf"
            | "-movflags"
            | "-muxdelay"
            | "-muxpreload"
            | "-max_interleave_delta"
            | "-copypriorss:v:0"
            | "-sc_threshold"
            | "-muxrate"
            | "-pcr_period"
            | "-t"
            | "-f"
            | "-mpegts_flags"
            | "-ss"
            | "-profile:v"
            | "-low_power"
            | "-look_ahead"
            | "-tag:v"
            | "-x265-params"
            | "-b_ref_mode"
            | "-preset:v"
            | "-tune:v"
            | "-num_capture_buffers"
            | "-rc_mode"
            | "-sei"
            | "-pix_fmt"
            | "-ac"
            | "-output_ts_offset"
            | "-b:a"
            | "-b:v"
            | "-maxrate:a"
            | "-maxrate:v"
            | "-bufsize:a"
            | "-bufsize:v"
            | "-ar"
            | "-r"
            | "-fps_mode:v"
            | "-video_track_timescale"
            | "-map_metadata"
            | "-map_metadata:g"
            | "-metadata"
            | "-metadata:s:a:0"
            | "-metadata:s:s:0"
            | "-filter_complex" => {
                if options.next().is_none() {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

fn digest(arguments: &[String]) -> Option<[u8; 32]> {
    let mut hash = Sha256::new();
    hash.update(b"ErsatzRS.Ffmpeg.GeneratedMpegTs.Arguments.v1");
    hash.update(u64::try_from(arguments.len()).ok()?.to_be_bytes());
    for argument in arguments {
        hash.update(u64::try_from(argument.len()).ok()?.to_be_bytes());
        hash.update(argument.as_bytes());
    }
    Some(hash.finalize().into())
}

#[cfg(test)]
#[path = "command_output_policy_tests.rs"]
mod tests;
