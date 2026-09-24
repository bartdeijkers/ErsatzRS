//! Startup-only discovery of the optional Next engine version.

use std::{path::Path, process::Stdio, time::Duration};

use tokio::{io::AsyncReadExt, process::Command};

use crate::addons::spawn_managed_addon_process;

const OUTPUT_LIMIT: u16 = 4096;
const TIMEOUT: Duration = Duration::from_secs(5);

/// Query the same executable used for playback. Unavailable or unsuccessful
/// probes return `None`; startup never requires the optional engine.
/// Output is capped at 4 KiB, execution at five seconds, and cleanup gets a
/// further five seconds. Cancellation terminates the managed process tree.
pub async fn discover() -> Option<String> {
    let binary = super::host::channel_binary().ok()?;
    probe(&binary, TIMEOUT).await
}

async fn probe(binary: &Path, timeout: Duration) -> Option<String> {
    let mut command = Command::new(binary);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = spawn_managed_addon_process(command).ok()?;
    let stdout = child.stdout().take()?;
    let mut output = Vec::new();
    let result = tokio::time::timeout(timeout, async {
        stdout
            .take(u64::from(OUTPUT_LIMIT) + 1)
            .read_to_end(&mut output)
            .await?;
        if output.len() > usize::from(OUTPUT_LIMIT) {
            return Err(std::io::Error::other("Next version output exceeds limit"));
        }
        child.wait().await
    })
    .await;
    match result {
        Ok(Ok(status)) if status.success() => parse(&output),
        Ok(Ok(_)) => None,
        _ => {
            let _ = tokio::time::timeout(TIMEOUT, child.kill()).await;
            None
        }
    }
}

fn parse(output: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(output).ok()?.trim();
    let version = text.strip_prefix("ersatztv-channel").unwrap_or(text).trim();
    (!version.is_empty() && !version.chars().any(char::is_control)).then(|| version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_version_output() {
        assert_eq!(
            parse(b"ersatztv-channel 0.1.0\r\n").as_deref(),
            Some("0.1.0")
        );
        assert_eq!(
            parse(b"v1.2.3-f6350a0\n").as_deref(),
            Some("v1.2.3-f6350a0")
        );
        for output in [
            b"".as_slice(),
            b"ersatztv-channel\n",
            b"\xff",
            b"a\nb",
            b"v1\x1b[0m",
        ] {
            assert_eq!(parse(output), None);
        }
    }

    // Shell fixtures exercise process failure and cleanup on Unix. Windows
    // uses the same probe with the existing managed Job Object boundary.
    #[cfg(unix)]
    #[tokio::test]
    async fn next_version_process_bounds() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("next");
        assert_eq!(probe(&binary, TIMEOUT).await, None);
        for (script, expected) in [
            ("printf 'ersatztv-channel 0.1.0\\n'", Some("0.1.0")),
            ("printf 'ersatztv-channel 0.1.0\\n'; exit 1", None),
            ("while :; do printf '0123456789'; done", None),
            ("sleep 30", None),
        ] {
            std::fs::write(&binary, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
            let result = tokio::time::timeout(
                Duration::from_secs(6),
                probe(&binary, Duration::from_millis(250)),
            )
            .await
            .expect("probe and process cleanup are bounded");
            assert_eq!(result.as_deref(), expected, "{script}");
        }
    }
}
