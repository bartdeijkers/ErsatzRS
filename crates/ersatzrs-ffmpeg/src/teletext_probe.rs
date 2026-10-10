//! Bounded decoder proof using original DVB fixtures (ADR 0121, P9A-02).
//!
//! This proves decoding, not the authored-page encoder or transport adapter.

use std::{io, process::Stdio, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{io::AsyncReadExt as _, io::AsyncWriteExt as _, process::Command};

/// Original synthetic full-page and subtitle packets, independently generated.
pub const TELETEXT_DECODE_FIXTURE: &[u8] = include_bytes!("fixtures/teletext.ts");
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_HELP_BYTES: usize = 64 * 1024;
const BITMAP_BYTES: usize = 492 * 250 * 3;
const REQUIRED_OPTIONS: &[&str] = &[
    "txt_page",
    "txt_default_region",
    "txt_chop_top",
    "txt_format",
    "txt_chop_spaces",
    "txt_duration",
    "txt_transparent",
    "txt_opacity",
];

/// Operation-scoped outcome; unavailable teletext does not reject other playback.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextDecoderStatus {
    /// The selected runtime does not advertise libzvbi teletext decoding.
    #[default]
    MissingDecoder,
    /// Decoder help lacks options required by the accepted page contract.
    MissingOptions,
    /// The bounded decoder-help process could not be completed.
    HelpProbeFailed,
    /// Full-page/subtitle text was missing, mixed with another page or undecodable.
    TextProbeFailed,
    /// The selected full-page bitmap did not preserve the expected colours.
    BitmapProbeFailed,
    /// Options and independent text/bitmap decoding both passed.
    Ready,
}

/// Functional result attached to the enclosing exact-runtime capability report.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextDecoderCapability {
    /// The strongest completed operation-scoped outcome.
    pub status: TeletextDecoderStatus,
    /// Required option names absent from decoder help, with no raw tool output.
    pub missing_options: Vec<String>,
}

impl TeletextDecoderCapability {
    /// Whether exact page selection, text and bitmap decode have been proven.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.status == TeletextDecoderStatus::Ready
    }
}

pub(crate) const TEXT_PROBE_ARGUMENTS: &[&str] = &[
    "-hide_banner",
    "-loglevel",
    "error",
    "-txt_page",
    "200,888",
    "-txt_format",
    "text",
    "-txt_chop_top",
    "0",
    "-txt_duration",
    "1000",
    "-f",
    "mpegts",
    "-i",
    "pipe:0",
    "-map",
    "0:s:0",
    "-c:s",
    "srt",
    "-f",
    "srt",
    "pipe:1",
];
pub(crate) const BITMAP_PROBE_ARGUMENTS: &[&str] = &[
    "-hide_banner",
    "-loglevel",
    "error",
    "-f",
    "lavfi",
    "-i",
    "color=c=black:s=492x250:r=25",
    "-txt_page",
    "200",
    "-txt_format",
    "bitmap",
    "-txt_chop_top",
    "0",
    "-f",
    "mpegts",
    "-i",
    "pipe:0",
    "-filter_complex",
    "[0:v][1:s:0]overlay,select=gte(t\\,1)",
    "-t",
    "2",
    "-frames:v",
    "1",
    "-pix_fmt",
    "rgb24",
    "-f",
    "rawvideo",
    "pipe:1",
];

/// Prove teletext decoder options and decoded text/colour from a synthetic stream.
///
/// Each process has a ten-second deadline and bounded stdin/stdout/stderr. Missing
/// or failed optional support is returned as data; it cannot fail unrelated startup.
/// The caller must bind the result to its selected runtime identity and generation.
pub async fn probe_teletext_decoder(ffmpeg: &str, advertised: bool) -> TeletextDecoderCapability {
    if !advertised {
        return TeletextDecoderCapability::default();
    }
    let Ok(help) = run_probe(
        ffmpeg,
        &["-hide_banner", "-h", "decoder=libzvbi_teletextdec"],
        &[],
        MAX_HELP_BYTES,
    )
    .await
    else {
        return outcome(TeletextDecoderStatus::HelpProbeFailed);
    };
    let missing_options = missing_decoder_options(&String::from_utf8_lossy(&help));
    if !missing_options.is_empty() {
        return TeletextDecoderCapability {
            status: TeletextDecoderStatus::MissingOptions,
            missing_options,
        };
    }
    let Ok(text) = run_probe(
        ffmpeg,
        TEXT_PROBE_ARGUMENTS,
        TELETEXT_DECODE_FIXTURE,
        MAX_HELP_BYTES,
    )
    .await
    else {
        return outcome(TeletextDecoderStatus::TextProbeFailed);
    };
    if !valid_decoded_text(&text) {
        return outcome(TeletextDecoderStatus::TextProbeFailed);
    }
    let Ok(bitmap) = run_probe(
        ffmpeg,
        BITMAP_PROBE_ARGUMENTS,
        TELETEXT_DECODE_FIXTURE,
        BITMAP_BYTES,
    )
    .await
    else {
        return outcome(TeletextDecoderStatus::BitmapProbeFailed);
    };
    if !valid_decoded_bitmap(&bitmap) {
        return outcome(TeletextDecoderStatus::BitmapProbeFailed);
    }
    outcome(TeletextDecoderStatus::Ready)
}

fn outcome(status: TeletextDecoderStatus) -> TeletextDecoderCapability {
    TeletextDecoderCapability {
        status,
        missing_options: Vec::new(),
    }
}

fn missing_decoder_options(help: &str) -> Vec<String> {
    let options: std::collections::BTreeSet<_> = help
        .lines()
        .filter_map(|line| {
            line.trim_start()
                .strip_prefix('-')?
                .split_whitespace()
                .next()
        })
        .collect();
    REQUIRED_OPTIONS
        .iter()
        .filter(|option| !options.contains(**option))
        .map(|option| (*option).to_owned())
        .collect()
}

fn valid_decoded_text(text: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(text) else {
        return false;
    };
    text.contains("TV GUIDE INDEX")
        && text.contains("Synthetic subtitle page 888")
        && text.contains("Independent decoder fixture")
        && text.contains("00:00:00,200")
        && !text.contains("CHANNEL ONE")
}

fn valid_decoded_bitmap(bitmap: &[u8]) -> bool {
    if bitmap.len() != BITMAP_BYTES {
        return false;
    }
    let mut cyan = 0;
    let mut green = 0;
    for pixel in bitmap.chunks_exact(3) {
        if pixel[0] < 80 && pixel[1] > 160 && pixel[2] > 160 {
            cyan += 1;
        }
        if pixel[0] < 80 && pixel[1] > 160 && pixel[2] < 80 {
            green += 1;
        }
    }
    cyan > 10 && green > 10
}

async fn bounded_read(
    reader: impl tokio::io::AsyncRead + Unpin,
    limit: usize,
) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "teletext probe output limit",
        ));
    }
    Ok(bytes)
}

pub(crate) async fn run_probe(
    ffmpeg: &str,
    args: &[&str],
    input: &[u8],
    limit: usize,
) -> io::Result<Vec<u8>> {
    let mut child = Command::new(ffmpeg)
        .args(args)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("missing probe stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("missing probe stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("missing probe stderr"))?;
    let work = async {
        // Own stdin so completion drops the pipe and delivers EOF before waiting.
        let write = async move {
            stdin.write_all(input).await?;
            stdin.shutdown().await
        };
        let ((), stdout, _, status) = tokio::try_join!(
            write,
            bounded_read(stdout, limit),
            bounded_read(stderr, MAX_HELP_BYTES),
            child.wait()
        )?;
        if !status.success() {
            return Err(io::Error::other("teletext probe process failed"));
        }
        Ok(stdout)
    };
    match tokio::time::timeout(PROBE_TIMEOUT, work).await {
        Ok(Ok(stdout)) => Ok(stdout),
        result => {
            let _ = child.kill().await;
            match result {
                Ok(Err(error)) => Err(error),
                _ => Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "teletext probe deadline",
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    #[test]
    fn decoder_probe_argv_snapshot() {
        // ADR 0121: exact-page, input-scoped decoder options and bounded output.
        insta::assert_snapshot!("teletext_text_probe_argv", TEXT_PROBE_ARGUMENTS.join("\n"));
    }

    #[test]
    fn bitmap_probe_argv_snapshot() {
        // The first input is the video canvas; page options belong to input 1.
        insta::assert_snapshot!(
            "teletext_bitmap_probe_argv",
            BITMAP_PROBE_ARGUMENTS.join("\n")
        );
    }

    #[tokio::test]
    async fn missing_decoder_does_not_spawn_or_fail_other_playback() {
        let result = probe_teletext_decoder("nonexistent-teletext-probe", false).await;
        assert_eq!(result.status, TeletextDecoderStatus::MissingDecoder);
    }

    #[tokio::test]
    async fn unavailable_executable_reports_failed_optional_probe() {
        let result = probe_teletext_decoder("nonexistent-teletext-probe", true).await;
        assert_eq!(result.status, TeletextDecoderStatus::HelpProbeFailed);
    }

    #[test]
    fn option_names_must_be_actual_decoder_options() {
        assert_eq!(
            missing_decoder_options("mention txt_page in prose").len(),
            REQUIRED_OPTIONS.len()
        );
        let mut help = String::new();
        for name in REQUIRED_OPTIONS {
            writeln!(help, "  -{name} <value> options").unwrap();
        }
        assert!(missing_decoder_options(&help).is_empty());
    }

    #[test]
    fn text_proof_requires_both_page_and_subtitle_and_excludes_unselected_page() {
        let decoded =
            b"00:00:00,200 TV GUIDE INDEX Synthetic subtitle page 888 Independent decoder fixture";
        assert!(valid_decoded_text(decoded));
        assert!(!valid_decoded_text(b"TV GUIDE INDEX"));
        let mut wrong_page = decoded.to_vec();
        wrong_page.extend_from_slice(b" CHANNEL ONE");
        assert!(!valid_decoded_text(&wrong_page));
    }

    #[test]
    fn bitmap_proof_rejects_blank_wrong_size_and_monochrome_output() {
        assert!(!valid_decoded_bitmap(&[]));
        assert!(!valid_decoded_bitmap(&vec![0; BITMAP_BYTES]));
        assert!(!valid_decoded_bitmap(&vec![255; BITMAP_BYTES]));
    }

    #[tokio::test]
    async fn captured_tool_output_is_bounded() {
        assert!(bounded_read(&b"oversized"[..], 4).await.is_err());
        assert_eq!(bounded_read(&b"data"[..], 4).await.unwrap(), b"data");
    }

    #[tokio::test]
    #[ignore = "requires explicit ERSATZRS_TELETEXT_TEST_FFMPEG pointing to the FFmpeg 9 candidate"]
    async fn real_runtime_decodes_exact_pages_subtitles_and_colours() {
        let ffmpeg = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_FFMPEG"])
            .expect("select the exact runtime candidate");
        let result = probe_teletext_decoder(&ffmpeg, true).await;
        assert_eq!(result.status, TeletextDecoderStatus::Ready, "{result:?}");
    }
}
