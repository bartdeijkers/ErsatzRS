use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::FrameSize;

#[derive(Debug, thiserror::Error)]
pub enum GraphicsTemplateError {
    #[error("graphics template has no file name: {0}")]
    MissingFileName(String),
    #[error("graphics template I/O error for {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawvgOverlay {
    pub svg_path: PathBuf,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub enable: Option<String>,
}

impl DrawvgOverlay {
    #[must_use]
    pub fn filter(&self) -> String {
        let mut filter = format!(
            "drawvg=file='{}':x={}:y={}:w={}:h={}",
            escape_path(&self.svg_path),
            self.x,
            self.y,
            self.width,
            self.height
        );
        if let Some(enable) = &self.enable {
            filter.push_str(":enable='");
            filter.push_str(&escape_filter_value(enable));
            filter.push('\'');
        }
        filter
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgTemplate {
    source: String,
}

impl SvgTemplate {
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    #[must_use]
    pub fn render(&self, variables: &BTreeMap<String, String>) -> String {
        let mut rendered = self.source.clone();
        for (name, value) in variables {
            rendered = rendered.replace(&format!("{{{{{name}}}}}"), &escape_xml(value));
        }
        rendered
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsTemplateRenderer {
    output_dir: PathBuf,
}

impl GraphicsTemplateRenderer {
    #[must_use]
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
        }
    }

    /// Renders a template file into the renderer output directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the template cannot be read, the output directory
    /// cannot be created, the template path has no file name, or the rendered
    /// SVG cannot be written.
    pub fn render_template_file(
        &self,
        template_path: impl AsRef<Path>,
        variables: &BTreeMap<String, String>,
    ) -> Result<PathBuf, GraphicsTemplateError> {
        let template_path = template_path.as_ref();
        let template =
            std::fs::read_to_string(template_path).map_err(|source| GraphicsTemplateError::Io {
                path: template_path.to_path_buf(),
                source,
            })?;
        let rendered = SvgTemplate::new(template).render(variables);

        std::fs::create_dir_all(&self.output_dir).map_err(|source| GraphicsTemplateError::Io {
            path: self.output_dir.clone(),
            source,
        })?;

        let file_name = template_path.file_name().ok_or_else(|| {
            GraphicsTemplateError::MissingFileName(template_path.display().to_string())
        })?;
        let output_path = self.output_dir.join(file_name);
        std::fs::write(&output_path, rendered).map_err(|source| GraphicsTemplateError::Io {
            path: output_path.clone(),
            source,
        })?;
        Ok(output_path)
    }
}

#[must_use]
pub fn lower_third_overlay(
    svg_path: impl Into<PathBuf>,
    frame_size: FrameSize,
    enable: Option<String>,
) -> DrawvgOverlay {
    DrawvgOverlay {
        svg_path: svg_path.into(),
        x: 0,
        y: i32::try_from(frame_size.height.saturating_mul(3) / 4).unwrap_or(i32::MAX),
        width: frame_size.width,
        height: frame_size.height / 4,
        enable,
    }
}

fn escape_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
}

fn escape_filter_value(value: &str) -> String {
    value.replace('\'', "\\'")
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
