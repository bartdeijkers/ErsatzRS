//! Actual render proof complements argv snapshots. Synthetic assets only.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ersatzrs_ffmpeg::DrawvgOverlay;

#[test]
#[ignore = "Requires an explicitly selected FFmpeg 9 with drawvg"]
fn native_drawvg_paths_render_complete_marker_frames() {
    let ffmpeg = selected_ffmpeg();
    let root = tempfile::tempdir().unwrap();
    let names = [
        "plate.vgs",
        "director's [cut], next; title.vgs",
        "café.vgs",
        " plate.vgs ",
        // Windows forbids ':' and treats backslashes as directory separators;
        // its native cases are exercised by windows-drawvg-paths.ps1.
        #[cfg(not(windows))]
        "colon:plate.vgs",
        #[cfg(not(windows))]
        r"literal\backslash.vgs",
    ];
    for name in names {
        let path = root.path().join(name);
        std::fs::write(&path, "setcolor red\nrect 8 8 24 24 fill\n").unwrap();
        let filter = DrawvgOverlay {
            script_path: if name == " plate.vgs " {
                name.into()
            } else {
                path
            },
            x: 0,
            y: 0,
            width: 64,
            height: 64,
            enable: None,
        }
        .filter();
        let frame_path = root.path().join("decoded.rgb");
        let stderr_path = root.path().join("stderr.txt");
        let stderr = std::fs::File::create(&stderr_path).unwrap();
        let mut child = Command::new(&ffmpeg)
            .current_dir(root.path())
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:r=1",
                "-vf",
                &filter,
                "-frames:v",
                "1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
            ])
            .arg(&frame_path)
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("drawvg path render exceeded the 15-second bound");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(
            status.success(),
            "{filter}: {}",
            std::fs::read_to_string(&stderr_path).unwrap()
        );
        let bytes = std::fs::read(frame_path).unwrap();
        assert_eq!(bytes.len(), 64 * 64 * 3);
        let red = &bytes[(16 * 64 + 16) * 3..][..3];
        let blue = &bytes[(48 * 64 + 48) * 3..][..3];
        assert!(
            red[0] > 230 && red[1] < 25 && red[2] < 25,
            "marker: {red:?}"
        );
        assert!(
            blue[2] > 230 && blue[0] < 25 && blue[1] < 25,
            "background: {blue:?}"
        );
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "Opt-in render proof selects a verified executable; production uses runtime configuration"
)]
fn selected_ffmpeg() -> std::ffi::OsString {
    std::env::var_os("FFMPEG_PATH").expect("set FFMPEG_PATH to FFmpeg 9")
}
