#[macro_use]
#[path = "../../../tests/support/manifest_dir.rs"]
mod test_manifest;

use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::Duration;

use ersatzrs_ffmpeg::{DrawtextExpansion, DrawtextFilterBuilder, DrawtextFont, DrawtextTextSource};

const WIDTH: usize = 320;
const HEIGHT: usize = 100;
const FRAME_COUNT: usize = 12;

/// Manual package gate: one `FFmpeg` process must observe an atomic text-file
/// replacement and render different decoded pixels without rebuilding its
/// graph. Set `ERSATZRS_FFMPEG` to the packaged executable before running.
#[test]
#[ignore = "requires a packaged FFmpeg 9 runtime"]
// This ignored package gate deliberately receives the external executable from
// its runner rather than application configuration.
#[allow(clippy::disallowed_methods)]
fn packaged_drawtext_reloads_text_in_the_same_process() {
    let ffmpeg = std::env::var_os("ERSATZRS_FFMPEG")
        .expect("ERSATZRS_FFMPEG must point to packaged FFmpeg 9");
    let root = tempfile::tempdir().expect("temporary runtime proof directory");
    let text_path = root.path().join("dynamic text ü.txt");
    let output_path = root.path().join("decoded frames.rgb");
    let font_path = test_manifest_dir!().join("../../assets/fonts/Roboto-Regular.ttf");
    std::fs::write(&text_path, "FIRST").expect("initial text file");

    let filter = DrawtextFilterBuilder::new(
        DrawtextTextSource::text_file(&text_path, std::num::NonZeroU32::new(1)),
        DrawtextFont::file(font_path),
    )
    .expansion(DrawtextExpansion::None)
    .font_size("40")
    .position("10", "30")
    .font_color("white")
    .text_shaping(true)
    .build();
    let child = Command::new(ffmpeg)
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=320x100:d=3:r=4",
            "-vf",
            &filter,
            "-frames:v",
            "12",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
        ])
        .arg(&output_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn packaged FFmpeg");
    let original_pid = child.id();

    std::thread::sleep(Duration::from_millis(1_300));
    let mut replacement =
        tempfile::NamedTempFile::new_in(root.path()).expect("same-directory replacement");
    replacement
        .write_all(b"SECOND")
        .expect("write replacement text");
    replacement.flush().expect("flush replacement text");
    replacement
        .persist(&text_path)
        .expect("atomically replace live text file");

    let output = child.wait_with_output().expect("wait for packaged FFmpeg");
    assert!(
        output.status.success(),
        "FFmpeg failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(original_pid, 0, "spawned process must have a real PID");

    let frames = std::fs::read(&output_path).expect("decoded RGB frames");
    let frame_bytes = WIDTH * HEIGHT * 3;
    assert_eq!(frames.len(), frame_bytes * FRAME_COUNT);
    let early = &frames[frame_bytes..frame_bytes * 2];
    let late = &frames[frame_bytes * 10..frame_bytes * 11];
    assert_ne!(early, late, "late frame must contain refreshed text pixels");
    assert!(early.iter().any(|value| *value != 0));
    assert!(late.iter().any(|value| *value != 0));
}
