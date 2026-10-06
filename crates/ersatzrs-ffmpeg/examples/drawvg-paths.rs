//! Emit production-generated filters for the isolated Windows drawvg gate.
use std::io::Write as _;

use ersatzrs_ffmpeg::{
    DrawtextExpansion, DrawtextFilterBuilder, DrawtextFont, DrawtextTextSource, DrawvgOverlay,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .ok_or("provide a temporary runtime directory")?;
    let font = std::env::args()
        .nth(2)
        .ok_or("provide a runtime font file")?;
    let mut output = std::io::stdout().lock();
    for filename in [
        "plate.vgs",
        "overlay files/director's [cut], next; title.vgs",
        "café.vgs",
    ] {
        for backslashes in [false, true] {
            let path = format!("{}/{filename}", root.trim_end_matches(['/', '\\']));
            let path = if backslashes {
                path.replace('/', "\\")
            } else {
                path
            };
            let overlay = DrawvgOverlay {
                script_path: path.clone().into(),
                x: 0,
                y: 0,
                width: 64,
                height: 64,
                enable: Some("between(t,0,1)".into()),
            };
            let text = DrawtextFilterBuilder::new(
                DrawtextTextSource::text_file(
                    format!("{root}/label's [cut].txt"),
                    std::num::NonZeroU32::new(1),
                ),
                DrawtextFont::file(&font),
            )
            .expansion(DrawtextExpansion::None)
            .font_size("12")
            .font_color("white")
            .position("40", "4")
            .build();
            writeln!(output, "{path}\t{},{}", overlay.filter(), text)?;
        }
    }
    Ok(())
}
