//! Report-only teletext metadata from the already-owned producer's copied prefix.

use std::io::Write as _;

use super::{
    parse_ffprobe_json_with_teletext,
    teletext_probe::{TeletextProbeEncoding, TeletextProbeSnapshot},
    FfprobeError, FfprobeExtractor, FfprobeMediaVersion, Result,
};

impl FfprobeExtractor {
    /// Inspect ordinary tracks and exact reported teletext associations from the
    /// same bounded prefix, without opening or consuming the producer again.
    ///
    /// The caller retains the original producer and replays its complete prefix.
    /// This auxiliary snapshot is not observed page content, decoder readiness,
    /// an immutable source, a complete inventory or a source-clock grant. Join
    /// it to actual prefix PSI/PES/header observations before selecting a page.
    /// Any reported duration describes copied bytes, not the ongoing producer.
    /// The existing 64-KiB prefix cap and owned probe cancellation/cleanup apply.
    ///
    /// # Errors
    /// Rejects empty/oversized input, failed or malformed probe output, expired
    /// deadlines, cancellation and unconfirmed cleanup without exposing locators.
    pub async fn extract_producer_prefix_with_teletext_cancellable(
        &self,
        prefix: &[u8],
        deadline: tokio::time::Instant,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(FfprobeMediaVersion, TeletextProbeSnapshot)> {
        let label = "source caption producer prefix";
        if prefix.is_empty() || prefix.len() > 64 * 1024 {
            return Err(FfprobeError::InvalidOutput { path: label.into() });
        }
        let mut file = tempfile::NamedTempFile::new().map_err(|source| FfprobeError::Spawn {
            path: label.into(),
            source,
        })?;
        file.write_all(prefix)
            .map_err(|source| FfprobeError::Spawn {
                path: label.into(),
                source,
            })?;
        let mut args = [
            "-hide_banner",
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-show_programs",
            "-show_data",
            "-data_dump_format",
            "base64",
            "-of",
            "json",
            "-i",
        ]
        .map(str::to_owned)
        .to_vec();
        args.push(file.path().to_string_lossy().into_owned());
        let deadline =
            deadline.min(tokio::time::Instant::now() + std::time::Duration::from_secs(15));
        let (status, stdout, _stderr) = self
            .run_owned_probe_output(label, None, args, None, deadline, cancellation)
            .await?;
        if !status.success() {
            return Err(FfprobeError::Process {
                path: label.into(),
                status,
                stderr: "source caption prefix diagnostics redacted".into(),
            });
        }
        // A finite copied prefix can end inside a packet/PES, and FFprobe may
        // diagnose that tail. Neither success nor stderr grants page readiness;
        // the strict wire observers qualify complete evidence independently.
        parse_ffprobe_json_with_teletext(
            "producer-prefix",
            &stdout,
            TeletextProbeEncoding::Ffprobe9Base64,
        )
        .map_err(|_| FfprobeError::InvalidOutput { path: label.into() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_prefixes_never_start_a_probe() {
        let extractor = FfprobeExtractor::new("nonexistent-caption-prefix-probe");
        for prefix in [Vec::new(), vec![0; 64 * 1024 + 1]] {
            let (_cancel, cancellation) = tokio::sync::oneshot::channel();
            let error = extractor
                .extract_producer_prefix_with_teletext_cancellable(
                    &prefix,
                    tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                    cancellation,
                )
                .await
                .unwrap_err();
            assert!(matches!(error, FfprobeError::InvalidOutput { .. }));
            assert!(!error.to_string().contains("nonexistent"));
        }
    }

    #[tokio::test]
    async fn cancelled_or_expired_prefix_probes_never_open_the_executable() {
        let extractor = FfprobeExtractor::new("nonexistent-caption-prefix-probe");
        let prefix = [0x47; 188];
        let (cancel, cancellation) = tokio::sync::oneshot::channel();
        cancel.send(()).unwrap();
        assert!(matches!(
            extractor
                .extract_producer_prefix_with_teletext_cancellable(
                    &prefix,
                    tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                    cancellation,
                )
                .await,
            Err(FfprobeError::Cancelled { .. })
        ));
        let (_cancel, cancellation) = tokio::sync::oneshot::channel();
        assert!(matches!(
            extractor
                .extract_producer_prefix_with_teletext_cancellable(
                    &prefix,
                    tokio::time::Instant::now(),
                    cancellation,
                )
                .await,
            Err(FfprobeError::Timeout { .. })
        ));
    }
}
