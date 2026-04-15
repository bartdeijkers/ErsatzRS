use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcatPlaylist {
    pub scheme: String,
    pub host: String,
    pub channel_number: String,
    pub mode: String,
}

impl ConcatPlaylist {
    #[must_use]
    pub fn new(
        scheme: impl Into<String>,
        host: impl Into<String>,
        channel_number: impl Into<String>,
        mode: impl Into<String>,
    ) -> Self {
        Self {
            scheme: scheme.into(),
            host: host.into(),
            channel_number: channel_number.into(),
            mode: mode.into(),
        }
    }

    #[must_use]
    pub fn render(&self) -> String {
        let url = format!(
            "{}://{}/ffmpeg/stream/{}?mode={}",
            self.scheme, self.host, self.channel_number, self.mode
        );
        format!("ffconcat version 1.0\nfile {url}\nfile {url}\n")
    }

    /// Writes this playlist to `path`.
    ///
    /// # Errors
    ///
    /// Returns any filesystem error raised while writing the playlist file.
    pub fn write_to(&self, path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        let path = path.as_ref();
        std::fs::write(path, self.render())?;
        Ok(path.to_path_buf())
    }
}
