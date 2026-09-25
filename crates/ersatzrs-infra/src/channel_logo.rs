//! Generated channel branding shared by the SVG endpoint and playback fallback.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    process::Stdio,
    sync::OnceLock,
    time::Duration,
};

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_ffmpeg::{DrawtextExpansion, DrawtextFilterBuilder, DrawtextFont, DrawtextTextSource};
use sha2::{Digest, Sha256};
use tokio::{process::Command, sync::Mutex, time::Instant};

const FONT: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");
const BACKGROUND: &str = "setcolor #050505\nrect 0 0 320 180 fill\nsetcolor #009688\nrect 8 8 304 4 fill\nrect 8 168 304 4 fill\nrect 8 8 4 164 fill\nrect 308 8 4 164 fill\n";

/// Abbreviation used by generated channel logos, including empty-name fallback.
#[must_use]
pub fn label(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "TV".to_owned();
    }
    let mut words = trimmed.split_whitespace();
    let first = words.next().unwrap_or_default();
    if let Some(second) = words.next() {
        first
            .chars()
            .take(1)
            .chain(second.chars().take(1))
            .collect::<String>()
            .to_uppercase()
    } else {
        trimmed.chars().take(6).collect::<String>().to_uppercase()
    }
}

/// Render a cached PNG because the supported `FFmpeg` runtime does not decode SVG.
pub(crate) async fn generated_image(name: &str) -> Result<PathBuf> {
    render_cached(
        &crate::file_system_layout::streams_cache_folder().join("channel-logos"),
        name,
        Path::new(&ersatzrs_ffmpeg::ffmpeg_path()),
    )
    .await
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable("generated channel logo", "image preparation failed")
}

async fn cached_png(path: &Path) -> bool {
    use tokio::io::AsyncReadExt as _;
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return false;
    };
    let mut magic = [0; 8];
    file.read_exact(&mut magic).await.is_ok() && magic == *b"\x89PNG\r\n\x1a\n"
}

async fn render_cached(root: &Path, name: &str, ffmpeg: &Path) -> Result<PathBuf> {
    static RENDER: OnceLock<Mutex<()>> = OnceLock::new();
    let label = label(name);
    let mut digest = Sha256::new();
    digest.update(b"channel-logo-png-v1");
    digest.update(FONT);
    digest.update(label.as_bytes());
    let mut key = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(key, "{byte:02x}").expect("writing to a String cannot fail");
    }
    let output = root.join(format!("{key}.png"));
    if cached_png(&output).await {
        return Ok(output);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let _guard = tokio::time::timeout_at(deadline, RENDER.get_or_init(|| Mutex::new(())).lock())
        .await
        .map_err(|_| unavailable())?;
    if cached_png(&output).await {
        return Ok(output);
    }
    tokio::fs::create_dir_all(root)
        .await
        .map_err(|_| unavailable())?;
    let pending = tempfile::tempdir_in(root).map_err(|_| unavailable())?;
    tokio::fs::write(pending.path().join("background.vgs"), BACKGROUND)
        .await
        .map_err(|_| unavailable())?;
    tokio::fs::write(pending.path().join("label.txt"), label)
        .await
        .map_err(|_| unavailable())?;
    tokio::fs::write(pending.path().join("font.ttf"), FONT)
        .await
        .map_err(|_| unavailable())?;
    let mut command = Command::new(ffmpeg);
    command
        .current_dir(pending.path())
        .args(arguments())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child =
        crate::addons::spawn_managed_addon_process(command).map_err(|_| unavailable())?;
    let result = tokio::time::timeout_at(deadline, child.wait()).await;
    match result {
        Ok(Ok(status)) if status.success() => {}
        _ => {
            if !matches!(
                tokio::time::timeout(Duration::from_secs(5), child.kill()).await,
                Ok(Ok(()))
            ) {
                // Retain private files if process-tree termination is unconfirmed.
                let _ = pending.keep();
            }
            return Err(unavailable());
        }
    }
    let image = pending.path().join("logo.png");
    if !cached_png(&image).await {
        return Err(unavailable());
    }
    let bytes = tokio::fs::read(image).await.map_err(|_| unavailable())?;
    // Reuse the repository's atomic image promotion; readers never see partial PNGs.
    let destination = output.clone();
    tokio::task::spawn_blocking(move || {
        crate::addon_artwork::write_atomically(&destination, &bytes)
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())?;
    Ok(output)
}

fn arguments() -> Vec<String> {
    let text = DrawtextFilterBuilder::new(
        DrawtextTextSource::text_file("label.txt", None),
        DrawtextFont::file("font.ttf"),
    )
    .expansion(DrawtextExpansion::None)
    .font_size("46")
    .font_color("0xf5f5f5")
    .position("(w-text_w)/2", "104-ascent")
    .build();
    [
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        "color=c=0x050505:s=320x180:r=1",
        "-vf",
        &format!("drawvg=file=background.vgs,{text}"),
        "-frames:v",
        "1",
        "-threads",
        "1",
        "-c:v",
        "png",
        "logo.png",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_logo_label_and_command_keep_text_out_of_filters() {
        assert_eq!(label("Movie Channel"), "MC");
        assert_eq!(label("  "), "TV");
        assert_eq!(label("A&B"), "A&B");
        assert_eq!(label("Één Kanaal"), "ÉK");
        insta::assert_snapshot!(arguments().join("\n"));
    }

    #[tokio::test]
    #[ignore = "requires the FFmpeg 9 runtime; uses disposable generated images"]
    #[allow(
        clippy::disallowed_methods,
        reason = "Explicit test-only FFmpeg location; production uses the configured runtime locator."
    )]
    async fn real_generated_logo_png_is_atomic_and_reused() {
        let ffmpeg = std::env::var_os("ERSATZRS_NEXT_TEST_FFMPEG").expect("FFmpeg fixture path");
        let root = tempfile::tempdir().unwrap();
        let image = render_cached(root.path(), "Movie Channel", Path::new(&ffmpeg))
            .await
            .unwrap();
        assert!(cached_png(&image).await);
        let bytes = tokio::fs::read(&image).await.unwrap();
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 320);
        assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 180);
        let same = render_cached(
            root.path(),
            "Movie Channel",
            Path::new("unavailable-ffmpeg"),
        )
        .await
        .unwrap();
        assert_eq!(same, image);
        assert!(render_cached(
            root.path(),
            "Different Name",
            Path::new("unavailable-ffmpeg")
        )
        .await
        .is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
