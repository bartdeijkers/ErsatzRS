//! High-resolution late-cue fixture for Native and Next startup qualification.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    process::Command,
    time::{timeout, Instant},
};

#[path = "pgs.rs"]
pub(super) mod pgs;

/// No subtitle packet occurs in the initial five-second probe window.
pub async fn fixture(ffmpeg: &Path, root: &Path) -> PathBuf {
    let subtitle = root.join("late.nl.sup");
    std::fs::write(
        &subtitle,
        pgs::rectangles([1920, 1080], [1500, 940], 20..28),
    )
    .unwrap();
    let clip = root.join("4k-truehd-late-pgs.mkv");
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-copyts",
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=3840x2080:r=24:d=28",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=28",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=28",
            "-i",
        ])
        .arg(subtitle)
        .args([
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-map",
            "3:s",
            "-c:v",
            "libx264",
            "-threads",
            "2",
            "-preset",
            "ultrafast",
            "-crf",
            "28",
            "-c:a",
            "truehd",
            "-strict",
            "-2",
            "-c:s",
            "copy",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "language=nld",
            "-metadata:s:s:0",
            "language=nld",
            "-t",
            "28",
        ])
        .arg(&clip);
    run(command, root, "generate", Duration::from_secs(90)).await;
    clip
}

/// Join each owned process on success, failure and timeout; retain diagnostics.
pub async fn run(
    mut command: Command,
    root: &Path,
    name: &str,
    limit: Duration,
) -> (Vec<u8>, Duration) {
    let stdout = root.join(format!("{name}.stdout"));
    let stderr = root.join(format!("{name}.stderr"));
    command
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout).unwrap())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .kill_on_drop(true);
    let start = Instant::now();
    let mut child = command.spawn().unwrap();
    let status = if let Ok(status) = timeout(limit, child.wait()).await {
        status.unwrap()
    } else {
        child.kill().await.unwrap();
        child.wait().await.unwrap();
        panic!(
            "{name} exceeded {limit:?}; diagnostics retained at {}",
            root.display()
        );
    };
    assert!(
        status.success(),
        "{name} failed: {}",
        String::from_utf8_lossy(&std::fs::read(stderr).unwrap())
    );
    (std::fs::read(stdout).unwrap(), start.elapsed())
}

pub async fn assert_unknown_pgs(ffprobe: &Path, clip: &Path, root: &Path) {
    let mut command = Command::new(ffprobe);
    command
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(clip);
    let (bytes, _) = run(command, root, "source-probe", Duration::from_secs(30)).await;
    let probe: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let streams = probe["streams"].as_array().unwrap();
    assert_eq!(
        (streams[0]["width"].as_u64(), streams[0]["height"].as_u64()),
        (Some(3840), Some(2080))
    );
    assert_eq!(streams[1]["codec_name"], "truehd");
    assert_eq!(streams[2]["codec_name"], "truehd");
    assert_eq!(streams[2]["tags"]["language"], "nld");
    assert_eq!(streams[3]["codec_name"], "hdmv_pgs_subtitle");
    assert!(
        streams[3]["width"].is_null() && streams[3]["height"].is_null(),
        "fixture no longer has unknown PGS dimensions"
    );
}

pub async fn assert_picture_and_audio(
    ffmpeg: &Path,
    clip: &Path,
    root: &Path,
    name: &str,
    burned: bool,
) {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(clip)
        .args([
            "-ss",
            "0.5",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=320:180",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let (rgb, _) = run(
        command,
        root,
        &format!("{name}-picture"),
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(rgb.len(), 320 * 180 * 3);
    let white = rgb
        .chunks_exact(3)
        .filter(|pixel| pixel.iter().all(|value| *value > 180))
        .count();
    if burned {
        assert!(white > 5, "late PGS rectangle must be visible");
        let rectangle = &rgb[(158 * 320 + 255) * 3..(158 * 320 + 255) * 3 + 3];
        assert!(
            rectangle.iter().all(|value| *value > 180),
            "PGS rectangle is misplaced: {rectangle:?}"
        );
    } else {
        assert_eq!(
            white, 0,
            "no rectangle may appear before the first cue or with subtitles off"
        );
    }
    let blue = &rgb[(90 * 320 + 160) * 3..(90 * 320 + 160) * 3 + 3];
    assert!(
        blue[0] < 50 && blue[1] < 50 && blue[2] > 200,
        "programme picture changed: {blue:?}"
    );
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(clip)
        .args([
            "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ]);
    let (pcm, _) = run(
        command,
        root,
        &format!("{name}-audio"),
        Duration::from_secs(30),
    )
    .await;
    let samples: Vec<_> = pcm
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    let crossings = samples
        .windows(2)
        .filter(|pair| pair[0] <= 0 && pair[1] > 0)
        .count();
    let hz = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!(
        (820.0..940.0).contains(&hz),
        "selected Dutch audio changed: {hz}"
    );
}
