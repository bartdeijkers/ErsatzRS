//! Private, owned native concat input; no erased locator leaves this boundary.
use std::{fmt, io::Write as _, sync::Arc};

use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_ffmpeg::{InputFile, SubtitleMethod};
use tokio::time::Instant;

use super::{
    check_deadline, local_teletext, probe_error, FfprobeExtractor, SourceTeletextBitmapArtifact,
    SourceTeletextBitmapFragments, SourceTeletextPageDecodeError,
};

pub(crate) struct SourceTeletextBitmapPlaylist {
    fragments: Arc<SourceTeletextBitmapFragments>,
    _file: tempfile::NamedTempFile,
    input: InputFile,
}
impl fmt::Debug for SourceTeletextBitmapPlaylist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceTeletextBitmapPlaylist")
            .field("fragments", &self.fragments)
            .finish_non_exhaustive()
    }
}
impl SourceTeletextBitmapPlaylist {
    pub(crate) fn input(&self) -> &InputFile {
        &self.input
    }
    pub(crate) fn source(&self) -> Arc<super::DecodedLocalTeletextPage> {
        self.fragments.artifact.source.clone()
    }
}
impl SourceTeletextBitmapArtifact {
    /// Initial expected input only; actual playback must initialize a private playlist.
    pub(crate) fn carriage_input(&self) -> Result<InputFile, SourceTeletextPageDecodeError> {
        let path = self
            .file
            .path()
            .to_str()
            .ok_or(SourceTeletextPageDecodeError::InvalidOutput)?;
        let mut input = InputFile::image_subtitle(path, SubtitleMethod::Copy);
        input.allow_readrate = false;
        Ok(input)
    }
}
impl FfprobeExtractor {
    pub(crate) async fn prepare_local_teletext_bitmap_playlist(
        &self,
        fragments: Arc<SourceTeletextBitmapFragments>,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<SourceTeletextBitmapPlaylist, SourceTeletextPageDecodeError> {
        check_deadline(deadline)?;
        let retained = fragments.clone();
        let file = local_teletext::run_source_guard_owned(deadline, hold, move |stop| {
            let invalid = || crate::ffprobe::FfprobeError::InvalidOutput {
                path: "source teletext bitmap playlist".into(),
            };
            stop.check_source_guard()?;
            let source = &retained.artifact.source;
            if !source.guard.is_current(&source.original)
                || !source.guard.is_current(&source.resolved)
            {
                return Err(invalid());
            }
            let bytes = retained.recipe.render().into_bytes();
            if bytes.len() > 16 * 1024 {
                return Err(invalid());
            }
            let mut file = tempfile::Builder::new()
                .prefix("ersatzrs-teletext-playlist-")
                .suffix(".ffconcat")
                .tempfile()
                .map_err(|_| invalid())?;
            file.write_all(&bytes).map_err(|_| invalid())?;
            file.flush().map_err(|_| invalid())?;
            stop.check_source_guard()?;
            if !source.guard.is_current(&source.original)
                || !source.guard.is_current(&source.resolved)
            {
                return Err(invalid());
            }
            Ok(file)
        })
        .await
        .map_err(probe_error)?;
        let path = file
            .path()
            .to_str()
            .ok_or(SourceTeletextPageDecodeError::InvalidOutput)?;
        let mut input = InputFile::image_subtitle(path, SubtitleMethod::Copy);
        input.allow_readrate = false;
        input.input_options = vec![
            "-f".into(),
            "concat".into(),
            "-safe".into(),
            "0".into(),
            "-protocol_whitelist".into(),
            "file".into(),
        ];
        check_deadline(deadline)?;
        Ok(SourceTeletextBitmapPlaylist {
            fragments,
            _file: file,
            input,
        })
    }
}
