//! A terminal generated `FFmpeg` report, bound to actual forwarded media bytes.

use std::time::Duration;

pub(super) fn configure(request: &mut ersatzrs_ffmpeg::PipelineRequest, owned_finite: bool) {
    if owned_finite {
        request.state.report_progress = true;
        request.state.progress_output = ersatzrs_ffmpeg::FfmpegProgressOutput::Stderr;
    }
}

pub(super) fn completed_duration(raw: &str, written: u64) -> Option<Duration> {
    if written == 0 || raw.lines().last()? != "progress=end" {
        return None;
    }
    let start = raw.rfind("\nbitrate=").map_or(0, |index| index + 1);
    let block = &raw[start..];
    if !block.starts_with("bitrate=") {
        return None;
    }
    let (mut bytes, mut micros, mut legacy_micros, mut display) = (None, None, None, None);
    let mut seen = 0_u16;
    for line in block.lines() {
        let (key, value) = line.split_once('=')?;
        let flag = match key {
            "bitrate" => 1,
            "total_size" => {
                bytes = Some(decimal(value)?);
                2
            }
            "out_time_us" => {
                micros = Some(decimal(value)?);
                4
            }
            "out_time_ms" => {
                legacy_micros = Some(decimal(value)?);
                8
            }
            "out_time" => {
                display = Some(display_microseconds(value)?);
                16
            }
            "dup_frames" => 32,
            "drop_frames" => 64,
            "speed" => 128,
            "progress" if value == "end" => 256,
            _ => return None,
        };
        if seen & flag != 0 {
            return None;
        }
        seen |= flag;
    }
    let micros = micros?;
    (bytes == Some(written)
        && micros > 0
        && legacy_micros == Some(micros)
        && display == Some(micros))
    .then_some(Duration::from_micros(micros))
}

fn decimal(value: &str) -> Option<u64> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

fn display_microseconds(value: &str) -> Option<u64> {
    let (hours, rest) = value.split_once(':')?;
    let (minutes, rest) = rest.split_once(':')?;
    let (seconds, fraction) = rest.split_once('.')?;
    let (hours, minutes, seconds) = (decimal(hours)?, decimal(minutes)?, decimal(seconds)?);
    if minutes >= 60 || seconds >= 60 || fraction.len() != 6 {
        return None;
    }
    hours
        .checked_mul(3_600_000_000)?
        .checked_add(minutes.checked_mul(60_000_000)?)?
        .checked_add(seconds.checked_mul(1_000_000)?)?
        .checked_add(decimal(fraction)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = "frame=1500\nfps=0.00\nstream_0_0_q=-1.0\nbitrate=4949.6kbits/s\ntotal_size=37141468\nout_time_us=60032000\nout_time_ms=60032000\nout_time=00:01:00.032000\ndup_frames=0\ndrop_frames=0\nspeed=1600x\nprogress=end";

    #[test]
    fn completed_report_requires_exact_forwarded_bytes_and_consistent_clocks() {
        assert_eq!(
            completed_duration(REPORT, 37_141_468),
            Some(Duration::from_millis(60_032))
        );
        for written in [0, 37_141_467, 37_141_469] {
            assert_eq!(completed_duration(REPORT, written), None);
        }
        assert_eq!(
            completed_duration(
                &REPORT.replace("out_time_ms=60032000", "out_time_ms=60031"),
                37_141_468
            ),
            None
        );
        assert_eq!(
            completed_duration(
                &REPORT.replace("out_time=00:01:00.032000", "out_time=00:01:01.032000"),
                37_141_468
            ),
            None
        );
    }

    #[test]
    fn incomplete_negative_overflowing_or_ambiguous_reports_grant_no_clock() {
        for raw in [
            REPORT.replace("progress=end", "progress=continue"),
            REPORT.replace("out_time_us=60032000", "out_time_us=N/A"),
            REPORT.replace("out_time_us=60032000", "out_time_us=-1"),
            REPORT.replace("out_time_us=60032000", "out_time_us=18446744073709551616"),
            REPORT.replace(
                "total_size=37141468",
                "total_size=37141468\ntotal_size=37141468",
            ),
            REPORT.replace("speed=1600x", "unexpected=1"),
            format!("{REPORT}\nunconfirmed cleanup"),
            REPORT.replace("bitrate=4949.6kbits/s\n", ""),
        ] {
            assert_eq!(completed_duration(&raw, 37_141_468), None, "{raw}");
        }
    }

    #[test]
    fn last_actual_terminal_report_overrides_an_earlier_claim() {
        let old = REPORT.replace("progress=end", "progress=continue");
        let raw = format!("{old}\n{REPORT}");
        assert_eq!(
            completed_duration(&raw, 37_141_468),
            Some(Duration::from_millis(60_032))
        );
        let truncated = REPORT.split_once("total_size=").unwrap().1;
        assert_eq!(completed_duration(truncated, 37_141_468), None);
    }
}
