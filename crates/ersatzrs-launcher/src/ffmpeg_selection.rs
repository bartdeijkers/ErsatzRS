//! Windows colocated-tool command selection, also exercised by portable tests.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

pub(super) fn configure_colocated_tools(
    command: &mut Command,
    executable_dir: &Path,
    state_dir: &Path,
    explicit_ffmpeg: bool,
    explicit_ffprobe: bool,
) {
    // Bootstrap copies managed tools beside the server. Those exact copies are
    // not operator overrides: injecting them would bypass managed updates and
    // erase installed-tag Health reporting on the next launcher restart.
    if !explicit_ffmpeg
        && !explicit_ffprobe
        && colocated_pair_matches_managed(executable_dir, state_dir)
    {
        return;
    }
    for (key, name, explicit) in [
        ("ERSATZRS_FFMPEG__PATH", "ffmpeg.exe", explicit_ffmpeg),
        ("ERSATZRS_FFPROBE__PATH", "ffprobe.exe", explicit_ffprobe),
    ] {
        let path = executable_dir.join(name);
        if !explicit && path.is_file() {
            command.env(key, path);
        }
    }
}

fn colocated_pair_matches_managed(executable_dir: &Path, state_dir: &Path) -> bool {
    let managed = state_dir.join("ffmpeg");
    managed.join("current-tag").is_file()
        && ["ffmpeg.exe", "ffprobe.exe"].into_iter().all(|name| {
            files_are_identical(
                &executable_dir.join(name),
                &managed.join("current/bin").join(name),
            )
            .unwrap_or(false)
        })
}

fn files_are_identical(left: &Path, right: &Path) -> io::Result<bool> {
    let (mut left, mut right) = (File::open(left)?, File::open(right)?);
    let (left_meta, right_meta) = (left.metadata()?, right.metadata()?);
    if !left_meta.is_file()
        || !right_meta.is_file()
        || left_meta.len() == 0
        || left_meta.len() != right_meta.len()
    {
        return Ok(false);
    }
    // Bound memory and comparison work to the initial regular-file length;
    // neither a same-length custom tool nor a matching prefix proves identity.
    let (mut left_buffer, mut right_buffer) = ([0; 16_384], [0; 16_384]);
    let mut remaining = left_meta.len();
    while remaining > 0 {
        let count = usize::try_from(remaining.min(16_384)).map_err(io::Error::other)?;
        left.read_exact(&mut left_buffer[..count])?;
        right.read_exact(&mut right_buffer[..count])?;
        if left_buffer[..count] != right_buffer[..count] {
            return Ok(false);
        }
        remaining -= u64::try_from(count).map_err(io::Error::other)?;
    }
    let mut extra = [0; 1];
    Ok(left.read(&mut extra)? == 0 && right.read(&mut extra)? == 0)
}

#[cfg(test)]
mod tests {
    use super::configure_colocated_tools;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::process::Command;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("application");
        let state = root.path().join("state");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(state.join("ffmpeg/current/bin")).unwrap();
        for (name, bytes) in [("ffmpeg.exe", b"ffmpeg"), ("ffprobe.exe", b"probe!")] {
            std::fs::write(app.join(name), bytes).unwrap();
            std::fs::write(state.join("ffmpeg/current/bin").join(name), bytes).unwrap();
        }
        std::fs::write(state.join("ffmpeg/current-tag"), b"v2.0.7").unwrap();
        (root, app, state)
    }

    fn selected(command: &Command, key: &str) -> Option<OsString> {
        command
            .get_envs()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value.map(OsString::from))
    }

    #[test]
    fn managed_copies_do_not_override_runtime_acquisition() {
        let (_root, app, state) = fixture();
        let mut command = Command::new("synthetic-server");
        configure_colocated_tools(&mut command, &app, &state, false, false);
        assert_eq!(selected(&command, "ERSATZRS_FFMPEG__PATH"), None);
        assert_eq!(selected(&command, "ERSATZRS_FFPROBE__PATH"), None);
    }

    #[test]
    fn different_custom_tools_remain_explicit_including_late_same_length_changes() {
        for bytes in [b"custom".to_vec(), vec![42; 131_073]] {
            let (_root, app, state) = fixture();
            std::fs::write(state.join("ffmpeg/current/bin/ffmpeg.exe"), &bytes).unwrap();
            let mut custom = bytes;
            *custom.last_mut().unwrap() ^= 1;
            std::fs::write(app.join("ffmpeg.exe"), custom).unwrap();
            let mut command = Command::new("synthetic-server");
            configure_colocated_tools(&mut command, &app, &state, false, false);
            assert_eq!(
                selected(&command, "ERSATZRS_FFMPEG__PATH"),
                Some(app.join("ffmpeg.exe").into_os_string())
            );
            assert_eq!(
                selected(&command, "ERSATZRS_FFPROBE__PATH"),
                Some(app.join("ffprobe.exe").into_os_string())
            );
        }
    }

    #[test]
    fn untagged_pair_remains_an_operator_installation() {
        let (_root, app, state) = fixture();
        std::fs::remove_file(state.join("ffmpeg/current-tag")).unwrap();
        let mut command = Command::new("synthetic-server");
        configure_colocated_tools(&mut command, &app, &state, false, false);
        assert!(selected(&command, "ERSATZRS_FFMPEG__PATH").is_some());
        assert!(selected(&command, "ERSATZRS_FFPROBE__PATH").is_some());
    }

    #[test]
    fn incomplete_managed_pair_cannot_claim_the_colocated_tools() {
        let (_root, app, state) = fixture();
        std::fs::remove_file(state.join("ffmpeg/current/bin/ffprobe.exe")).unwrap();
        let mut command = Command::new("synthetic-server");
        configure_colocated_tools(&mut command, &app, &state, false, false);
        assert!(selected(&command, "ERSATZRS_FFMPEG__PATH").is_some());
        assert!(selected(&command, "ERSATZRS_FFPROBE__PATH").is_some());
    }

    #[test]
    fn explicit_environment_is_not_replaced_by_colocation() {
        let (_root, app, state) = fixture();
        let mut command = Command::new("synthetic-server");
        command.env("FFMPEG_PATH", "chosen-ffmpeg");
        command.env("FFPROBE_PATH", "chosen-ffprobe");
        configure_colocated_tools(&mut command, &app, &state, true, true);
        assert_eq!(selected(&command, "ERSATZRS_FFMPEG__PATH"), None);
        assert_eq!(selected(&command, "ERSATZRS_FFPROBE__PATH"), None);
        assert_eq!(
            selected(&command, "FFMPEG_PATH"),
            Some("chosen-ffmpeg".into())
        );
        assert_eq!(
            selected(&command, "FFPROBE_PATH"),
            Some("chosen-ffprobe".into())
        );
    }

    #[test]
    fn partial_explicit_choice_keeps_its_colocated_companion() {
        let (_root, app, state) = fixture();
        let mut command = Command::new("synthetic-server");
        command.env("FFMPEG_PATH", "chosen-ffmpeg");
        configure_colocated_tools(&mut command, &app, &state, true, false);
        assert_eq!(selected(&command, "ERSATZRS_FFMPEG__PATH"), None);
        assert_eq!(
            selected(&command, "ERSATZRS_FFPROBE__PATH"),
            Some(app.join("ffprobe.exe").into_os_string())
        );
    }

    #[test]
    fn absent_colocated_companion_is_not_manufactured() {
        let (_root, app, state) = fixture();
        std::fs::remove_file(app.join("ffprobe.exe")).unwrap();
        let mut command = Command::new("synthetic-server");
        configure_colocated_tools(&mut command, &app, &state, false, false);
        assert!(selected(&command, "ERSATZRS_FFMPEG__PATH").is_some());
        assert_eq!(selected(&command, "ERSATZRS_FFPROBE__PATH"), None);
    }
}
