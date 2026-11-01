use std::path::{Path, PathBuf};

mod source_bitmap;
pub use source_bitmap::{SourceBitmapConcat, SourceBitmapConcatError};

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
        self.render_internal(None)
    }

    /// Bind both finite item requests to one exact legacy wrapper lifetime.
    /// The numeric identity renders as a fixed hexadecimal UUID, so it cannot
    /// add playlist directives or other query parameters.
    #[must_use]
    pub fn render_with_wrapper_id(&self, wrapper_id: u128) -> String {
        self.render_internal(Some(wrapper_id))
    }

    fn render_internal(&self, wrapper_id: Option<u128>) -> String {
        let url = format!(
            "{}://{}/ffmpeg/stream/{}?mode={}",
            self.scheme, self.host, self.channel_number, self.mode
        );
        let url = match wrapper_id {
            Some(id) => format!("{url}&wrapper_id={id:032x}"),
            None => url,
        };
        // Programme responses are bounded, chunked HTTP streams. FFmpeg 9's
        // single-request reader closes the connection on the terminal chunk,
        // then reports EIO on its next read. Persistent request handling keeps
        // that normal EOF available to the concat demuxer so it opens the next
        // programme instead of aborting the channel.
        // For these internal MPEG-TS responses, skip only optional FPS sampling
        // on each input open (ADR 0121). Codec parameters, first DTS and complete
        // stream discovery remain required; no frame rate is forced.
        format!(
            "ffconcat version 1.0\nfile {url}\noption multiple_requests 1\noption fpsprobesize 0\nfile {url}\noption multiple_requests 1\noption fpsprobesize 0\n"
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_programme_playlist_preserves_chunked_eof_for_each_entry() {
        let playlist = ConcatPlaylist::new("http", "localhost", "1", "hybrid");
        insta::assert_snapshot!("legacy_internal_programme_playlist", playlist.render());
    }

    #[test]
    fn both_item_requests_retain_the_exact_wrapper_identity() {
        let playlist = ConcatPlaylist::new("http", "localhost", "1", "ts-legacy");
        let rendered = playlist.render_with_wrapper_id(0x1234);
        assert_eq!(
            rendered
                .matches("&wrapper_id=00000000000000000000000000001234")
                .count(),
            2
        );
        assert_eq!(rendered.matches("option multiple_requests 1").count(), 2);
        assert_eq!(rendered.matches("option fpsprobesize 0").count(), 2);
        assert!(!playlist.render().contains("wrapper_id"));
    }
}
