//! Completed `FFmpeg` speed from native logs or Next's immediate report folders.

use std::path::Path;

use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};

const REPORT_TAIL_BYTES: u64 = 4 * 1024 * 1024;
const MAX_REPORT_ENTRIES: usize = 256;

pub(super) async fn completed_speed(folder: &Path, native_log: &str) -> Option<f64> {
    // The caller holds preview ownership. Slow/malformed reports must not keep
    // the completion endpoint or a new preview waiting indefinitely.
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut speed = parse_speed(native_log);
        let mut entries = tokio::fs::read_dir(folder).await.ok()?;
        let mut directories: Vec<std::path::PathBuf> = Vec::new();
        for _ in 0..MAX_REPORT_ENTRIES {
            let Some(entry) = entries.next_entry().await.ok()? else {
                directories.sort();
                for directory in directories {
                    if let Some(value) = report_speed(&directory).await {
                        speed = Some(value);
                    }
                }
                return speed;
            };
            // Do not follow links or recursively traverse unrelated folders.
            if entry.file_type().await.ok()?.is_dir() {
                directories.push(entry.path().join("ffreport.log"));
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

async fn report_speed(path: &Path) -> Option<f64> {
    if !tokio::fs::symlink_metadata(path).await.ok()?.is_file() {
        return None;
    }
    let mut file = tokio::fs::File::open(path).await.ok()?;
    let length = file.metadata().await.ok()?.len();
    let start = length.saturating_sub(REPORT_TAIL_BYTES);
    file.seek(std::io::SeekFrom::Start(start)).await.ok()?;
    let mut bytes = Vec::new();
    file.take(REPORT_TAIL_BYTES)
        .read_to_end(&mut bytes)
        .await
        .ok()?;
    // Ignore a truncated first line; only complete report records are evidence.
    let bytes = if start > 0 {
        &bytes[bytes.iter().position(|byte| *byte == b'\n')? + 1..]
    } else {
        &bytes
    };
    parse_speed(&String::from_utf8_lossy(bytes))
}

pub(super) fn parse_speed(text: &str) -> Option<f64> {
    // Match upstream's case-insensitive speed=\s*([\d\.]+)x contract. Reject
    // malformed and non-finite values rather than rendering misleading numbers.
    let lowercase = text.to_ascii_lowercase();
    lowercase
        .split("speed=")
        .skip(1)
        .filter_map(|part| {
            let part = part.trim_start();
            let length = part
                .bytes()
                .take_while(|byte| byte.is_ascii_digit() || *byte == b'.')
                .count();
            if part.as_bytes().get(length) != Some(&b'x') {
                return None;
            }
            part[..length]
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
        })
        .last()
}

pub(super) fn speed_diagnostic(speed: f64) -> String {
    // Troubleshooting works ahead without playback throttling, as upstream does.
    let warning = if speed < 1.0 {
        tracing::error!(speed, "Troubleshooting transcode is too slow for playback");
        "ERROR: unthrottled transcode is not fast enough for playback"
    } else if speed <= 1.5 {
        tracing::warn!(
            speed,
            "Troubleshooting transcode may be too slow for playback"
        );
        "WARN: unthrottled transcode may not be fast enough for playback"
    } else {
        "INFO: unthrottled transcode completed"
    };
    format!("\n{warning}; speed={speed}x\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_valid_speed_ignores_missing_and_malformed_values() {
        assert_eq!(
            parse_speed("speed= 0.89x\rframe=10 SPEED=1.25x\nspeed=N/A"),
            Some(1.25)
        );
        for value in [
            "speed=NaNx",
            "speed=infx",
            "speed=-1x",
            "speed=1.2.3x",
            "speed=2",
            "",
        ] {
            assert_eq!(parse_speed(value), None);
        }
        assert_eq!(parse_speed("speed=0x"), Some(0.0));
    }

    #[tokio::test]
    async fn completed_speed_uses_only_immediate_reports_and_bounded_tail() {
        let root = tempfile::tempdir().unwrap();
        let report = root.path().join("001");
        tokio::fs::create_dir_all(report.join("deeper"))
            .await
            .unwrap();
        tokio::fs::write(report.join("deeper/ffreport.log"), "speed=999x")
            .await
            .unwrap();
        tokio::fs::write(root.path().join("ffreport.log"), "speed=888x")
            .await
            .unwrap();
        assert_eq!(completed_speed(root.path(), "speed=1.5x").await, Some(1.5));
        let mut bytes = vec![b'x'; usize::try_from(REPORT_TAIL_BYTES).unwrap() + 1];
        bytes.extend_from_slice(b"\nspeed=2.5x\nspeed=N/A\n");
        tokio::fs::write(report.join("ffreport.log"), bytes)
            .await
            .unwrap();
        assert_eq!(completed_speed(root.path(), "speed=1.5x").await, Some(2.5));
        let next = root.path().join("002");
        tokio::fs::create_dir(&next).await.unwrap();
        tokio::fs::write(next.join("ffreport.log"), "speed=0.5x")
            .await
            .unwrap();
        assert_eq!(completed_speed(root.path(), "").await, Some(0.5));
    }
}
