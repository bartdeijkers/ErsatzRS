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
    /// The exact runtime preserves every declared fixture PES timestamp with
    /// its opt-in input flag. Missing legacy report data grants no authority.
    #[serde(default)]
    pub original_pts: bool,
}

impl TeletextDecoderCapability {
    /// Whether exact page selection, text and bitmap decode have been proven.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.status == TeletextDecoderStatus::Ready
    }

    /// Whether both decoder and original Live PES-clock proofs succeeded.
    #[must_use]
    pub fn preserves_original_pts(&self) -> bool {
        self.is_ready() && self.original_pts
    }
}

pub(crate) const ORIGINAL_PTS_PROBE_ARGUMENTS: &[&str] = &[
    "-hide_banner",
    "-loglevel",
    "error",
    "-copyts",
    "-fflags",
    "+preserve_teletext_pts",
    "-f",
    "mpegts",
    "-i",
    "pipe:0",
    "-map",
    "0:s:0",
    "-c",
    "copy",
    "-f",
    "framecrc",
    "pipe:1",
];

// Skew only the independent fixture's adaptation-only PCRs by one second.
// Its ordinary PCRs do not trigger repair, so accepting that input alone could
// mistake a recognized-but-ignored flag for functional original-clock support.
fn original_pts_probe_fixture() -> Option<Vec<u8>> {
    let mut fixture = TELETEXT_DECODE_FIXTURE.to_vec();
    let mut changed = 0;
    for packet in fixture.chunks_exact_mut(188) {
        if packet[..3] != [0x47, 0x01, 0x00]
            || packet[3] & 0x30 != 0x20
            || packet[4] != 183
            || packet[5] != 0x10
        {
            continue;
        }
        let base = (u64::from(packet[6]) << 25)
            | (u64::from(packet[7]) << 17)
            | (u64::from(packet[8]) << 9)
            | (u64::from(packet[9]) << 1)
            | u64::from(packet[10] >> 7);
        let base = base + 90000;
        let extension = packet[10] & 0x7f;
        packet[6..11].copy_from_slice(&(base << 7).to_be_bytes()[3..]);
        packet[10] |= extension;
        changed += 1;
    }
    (changed == 56).then_some(fixture)
}

// The independent pinned fixture declares eight cycles of seven PES timestamps.
// Do not accept a flag name alone, a partial prefix or a retimed packet trace.
fn valid_original_pts(output: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(output) else {
        return false;
    };
    if !text.lines().any(|line| line == "#tb 0: 1/90000") {
        return false;
    }
    let mut packets = text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    for cycle in 0..8 {
        for offset in [0, 3600, 18000, 21600, 36000, 39600, 72000] {
            let Some(line) = packets.next() else {
                return false;
            };
            let mut fields = line.split(',').map(str::trim);
            if fields.next() != Some("0") {
                return false;
            }
            let expected = 90000 + cycle * 90000 + offset;
            for _ in 0..2 {
                if fields.next().and_then(|value| value.parse::<i64>().ok()) != Some(expected) {
                    return false;
                }
            }
        }
    }
    packets.next().is_none()
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
            original_pts: false,
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
    let mut capability = outcome(TeletextDecoderStatus::Ready);
    if let Some(fixture) = original_pts_probe_fixture() {
        capability.original_pts = run_probe(
            ffmpeg,
            ORIGINAL_PTS_PROBE_ARGUMENTS,
            &fixture,
            MAX_HELP_BYTES,
        )
        .await
        .is_ok_and(|output| valid_original_pts(&output));
    }
    capability
}

fn outcome(status: TeletextDecoderStatus) -> TeletextDecoderCapability {
    TeletextDecoderCapability {
        status,
        missing_options: Vec::new(),
        original_pts: false,
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
    fn original_pts_probe_argv_snapshot() {
        insta::assert_snapshot!(
            "teletext_original_pts_probe_argv",
            ORIGINAL_PTS_PROBE_ARGUMENTS.join("\n")
        );
    }

    #[test]
    fn original_pts_input_changes_only_the_56_pcr_fields_not_pes_or_payload() {
        use sha2::Digest as _;
        let fixture = original_pts_probe_fixture().unwrap();
        let expected = [
            0x08, 0xec, 0xfe, 0xde, 0xeb, 0xe5, 0x05, 0x4f, 0x70, 0x7a, 0x3d, 0xb2, 0xbd, 0xfc,
            0x82, 0xf7, 0x3b, 0xfa, 0xef, 0x0c, 0xf4, 0x4a, 0x62, 0xc5, 0x87, 0x02, 0x7d, 0x98,
            0xd3, 0x0e, 0x62, 0xf8,
        ];
        assert_eq!(&sha2::Sha256::digest(&fixture)[..], &expected);
        let mut changed = 0;
        for (original, prepared) in TELETEXT_DECODE_FIXTURE
            .chunks_exact(188)
            .zip(fixture.chunks_exact(188))
        {
            if original != prepared {
                assert_eq!(&original[..6], &prepared[..6]);
                assert_eq!(&original[11..], &prepared[11..]);
                assert_eq!(&original[..3], &[0x47, 0x01, 0x00]);
                assert_eq!(original[3] & 0x30, 0x20);
                changed += 1;
            }
        }
        assert_eq!(changed, 56);
        assert_eq!(fixture.len(), TELETEXT_DECODE_FIXTURE.len());
    }

    #[test]
    fn original_clock_requires_complete_exact_pts_dts_and_time_base() {
        let mut valid = String::from("#tb 0: 1/90000\n");
        for cycle in 0..8 {
            for offset in [0, 3600, 18000, 21600, 36000, 39600, 72000] {
                let pts = 90000 + cycle * 90000 + offset;
                writeln!(valid, "0, {pts}, {pts}, 0, 139, checksum").unwrap();
            }
        }
        assert!(valid_original_pts(valid.as_bytes()));
        for invalid in [
            valid.replace("90000, 90000", "89999, 90000"),
            valid.replace("90000, 90000", "90000, 89999"),
            valid.replace("1/90000", "1/1000"),
            valid.lines().take(56).collect::<Vec<_>>().join("\n"),
            format!("{valid}0, 90000, 90000, 0, 139, duplicate\n"),
        ] {
            assert!(!valid_original_pts(invalid.as_bytes()));
        }
    }

    #[test]
    fn legacy_report_does_not_authorize_original_live_timestamps() {
        let report: TeletextDecoderCapability =
            serde_json::from_str(r#"{"status":"ready","missing_options":[]}"#).unwrap();
        assert!(report.is_ready());
        assert!(!report.original_pts);
        assert!(!report.preserves_original_pts());
        let mut changed = report;
        changed.original_pts = true;
        assert!(changed.preserves_original_pts());
        changed.status = TeletextDecoderStatus::TextProbeFailed;
        assert!(!changed.preserves_original_pts());
    }

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
        if crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_EXPECT_ORIGINAL_PTS"]).is_some()
        {
            assert!(result.original_pts, "{result:?}");
        }
    }
}
