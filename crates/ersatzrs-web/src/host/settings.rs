use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};

use async_trait::async_trait;
use tokio::sync::RwLock;

use ersatzrs_app::{
    SettingsFfmpegPaths, SettingsRuntime, SettingsRuntimeError, WindowsStartupStatus,
};
use ersatzrs_core::repositories::{AdminSettingsRepository, SettingsMutation};
use ersatzrs_domain::EntityId;
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::db::DbPool;

#[derive(Clone)]
pub struct NativeSettingsRuntime {
    pool: DbPool,
    ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    tuner_count: Arc<AtomicI32>,
}

impl NativeSettingsRuntime {
    #[must_use]
    pub fn new(
        pool: DbPool,
        ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
        tuner_count: Arc<AtomicI32>,
    ) -> Self {
        Self {
            pool,
            ffmpeg_runtime,
            tuner_count,
        }
    }
}

#[async_trait]
impl SettingsRuntime for NativeSettingsRuntime {
    fn windows_startup_status(&self) -> WindowsStartupStatus {
        windows_startup_status()
    }

    fn set_windows_startup(&self, enabled: bool) -> Result<(), SettingsRuntimeError> {
        set_windows_startup(enabled)
    }

    async fn reload_logging(&self) -> Result<(), SettingsRuntimeError> {
        crate::host::bootstrap::reload_logging_from_database(&self.pool)
            .await
            .map_err(SettingsRuntimeError::unavailable)
    }

    async fn validate_ffmpeg_tools(
        &self,
        ffmpeg_path: &str,
        ffprobe_path: &str,
    ) -> Result<SettingsFfmpegPaths, SettingsRuntimeError> {
        let ffmpeg = ersatzrs_ffmpeg::resolve_ffmpeg_path(Some(ffmpeg_path)).path;
        let ffprobe = ersatzrs_ffmpeg::resolve_ffprobe_path(Some(ffprobe_path)).path;
        validate_tool(&ffmpeg, "ffmpeg").await?;
        validate_tool(&ffprobe, "ffprobe").await?;
        Ok(SettingsFfmpegPaths { ffmpeg, ffprobe })
    }

    async fn commit_ffmpeg_settings(
        &self,
        ffmpeg_path: &str,
        ffprobe_path: &str,
        mut mutations: Vec<SettingsMutation>,
    ) -> Result<(), SettingsRuntimeError> {
        let config = crate::host::bootstrap::ffmpeg_runtime_config_with_profile_probes(
            &self.pool,
            ffmpeg_path,
            ffprobe_path,
        )
        .await;
        let mut report = ersatzrs_ffmpeg::check_ffmpeg_runtime_with_config(&config)
            .await
            .map_err(|error| SettingsRuntimeError::unavailable(error.to_string()))?;

        // Saving FFmpeg settings is an explicit capability refresh (ADR 0014),
        // so learned graph denials are discarded and re-probed against the
        // runtime the operator just selected.
        ersatzrs_infra::hardware_graph::refresh_hardware_graph_capabilities(
            std::path::Path::new(ffmpeg_path),
            &mut report.capabilities,
        )
        .await;

        // Hold publication closed while the database transaction commits. A
        // failure leaves both the old settings and old runtime active.
        let mut active = self.ffmpeg_runtime.write().await;
        report.set_generation(active.generation.saturating_add(1));
        let graph_state = ersatzrs_infra::hardware_graph::encode_hardware_graph_state(
            ffmpeg_path,
            &report.capabilities,
        )
        .map_err(|error| SettingsRuntimeError::unavailable(error.to_string()))?;
        mutations.push(SettingsMutation::Delete {
            key: ersatzrs_infra::ffmpeg_capabilities::CONFIG_KEY_HARDWARE_DECODE_DENIALS.to_owned(),
        });
        mutations.push(SettingsMutation::Set {
            key: ersatzrs_infra::hardware_graph::CONFIG_KEY_HARDWARE_GRAPH_DENIALS.to_owned(),
            value: graph_state,
        });
        let repository =
            ersatzrs_infra::repositories::SqliteAdminSettingsRepository(self.pool.clone());
        repository
            .apply_mutations(mutations)
            .await
            .map_err(|error| SettingsRuntimeError::unavailable(error.to_string()))?;

        *active = report;
        ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg_path, ffprobe_path);
        Ok(())
    }

    fn request_embedded_subtitle_extraction(&self, playout_ids: Vec<EntityId>) {
        let pool = self.pool.clone();
        tokio::spawn(async move {
            for playout_id in playout_ids {
                if let Err(error) =
                    ersatzrs_infra::subtitles::extract_embedded_playout_subtitles(&pool, playout_id)
                        .await
                {
                    tracing::warn!(
                        playout_id,
                        %error,
                        "embedded subtitle extraction failed after ffmpeg settings save"
                    );
                }
            }
        });
    }

    fn ensure_ffmpeg_report_directory(&self) {
        let path = ersatzrs_infra::logging::default_log_dir().join("ffmpeg");
        if let Err(error) = std::fs::create_dir_all(&path) {
            tracing::warn!(%error, path = %path.display(), "failed to create ffmpeg report folder");
        }
    }

    fn set_tuner_count(&self, tuner_count: i32) {
        self.tuner_count.store(tuner_count, Ordering::Relaxed);
    }
}

#[cfg(not(windows))]
fn windows_startup_status() -> WindowsStartupStatus {
    WindowsStartupStatus::Unsupported
}

#[cfg(not(windows))]
fn set_windows_startup(_enabled: bool) -> Result<(), SettingsRuntimeError> {
    Err(SettingsRuntimeError::unavailable(
        "Windows startup registration is not supported on this platform",
    ))
}

#[cfg(any(windows, test))]
fn classify_windows_startup_status(
    expected_command: Option<&str>,
    registered_command: Option<&str>,
) -> WindowsStartupStatus {
    match (expected_command, registered_command) {
        (None, _) => WindowsStartupStatus::Unavailable,
        (Some(_), None) => WindowsStartupStatus::Disabled,
        (Some(expected), Some(registered)) if registered.trim().eq_ignore_ascii_case(expected) => {
            WindowsStartupStatus::Enabled
        }
        (Some(_), Some(_)) => WindowsStartupStatus::NeedsRepair,
    }
}

#[cfg(windows)]
mod windows_startup {
    use std::io;
    use std::path::PathBuf;

    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    use super::{classify_windows_startup_status, SettingsRuntimeError, WindowsStartupStatus};

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "ErsatzRS";
    const LAUNCHER_NAME: &str = "ErsatzRS.exe";

    pub(super) fn status() -> WindowsStartupStatus {
        let expected_command = launcher_path()
            .ok()
            .map(|launcher| launcher_command(&launcher));
        match read_registration() {
            Ok(registered_command) => classify_windows_startup_status(
                expected_command.as_deref(),
                registered_command.as_deref(),
            ),
            Err(error) => {
                tracing::warn!(%error, "failed to inspect Windows startup registration");
                WindowsStartupStatus::Unavailable
            }
        }
    }

    pub(super) fn set(enabled: bool) -> Result<(), SettingsRuntimeError> {
        let result = if enabled {
            let launcher = launcher_path()?;
            write_registration(&launcher_command(&launcher))
        } else {
            delete_registration()
        };

        result.map_err(|error| {
            tracing::warn!(%error, enabled, "failed to update Windows startup registration");
            SettingsRuntimeError::unavailable("Unable to update Windows startup registration")
        })
    }

    fn launcher_path() -> Result<PathBuf, SettingsRuntimeError> {
        let executable = std::env::current_exe().map_err(|error| {
            tracing::warn!(%error, "failed to resolve the ErsatzRS server executable");
            SettingsRuntimeError::unavailable("ErsatzRS.exe was not found next to ersatzrs-web.exe")
        })?;
        let launcher = executable
            .parent()
            .map(|directory| directory.join(LAUNCHER_NAME))
            .filter(|path| path.is_file())
            .ok_or_else(|| {
                SettingsRuntimeError::unavailable(
                    "ErsatzRS.exe was not found next to ersatzrs-web.exe",
                )
            })?;
        Ok(launcher)
    }

    fn launcher_command(path: &std::path::Path) -> String {
        format!(r#""{}""#, path.display())
    }

    fn read_registration() -> io::Result<Option<String>> {
        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let run = match current_user.open_subkey_with_flags(RUN_KEY, KEY_READ) {
            Ok(run) => run,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        read_registration_from(&run)
    }

    fn read_registration_from(run: &RegKey) -> io::Result<Option<String>> {
        match run.get_value(VALUE_NAME) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write_registration(command: &str) -> io::Result<()> {
        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let (run, _) = current_user.create_subkey(RUN_KEY)?;
        write_registration_to(&run, command)
    }

    fn write_registration_to(run: &RegKey, command: &str) -> io::Result<()> {
        run.set_value(VALUE_NAME, &command)
    }

    fn delete_registration() -> io::Result<()> {
        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let run = match current_user.open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE) {
            Ok(run) => run,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        delete_registration_from(&run)
    }

    fn delete_registration_from(run: &RegKey) -> io::Result<()> {
        match run.delete_value(VALUE_NAME) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            delete_registration_from, launcher_command, read_registration_from,
            write_registration_to,
        };
        use crate::host::settings::classify_windows_startup_status;
        use ersatzrs_app::WindowsStartupStatus;
        use std::path::Path;
        use std::time::{SystemTime, UNIX_EPOCH};
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        #[test]
        fn launcher_command_quotes_paths_with_spaces() {
            assert_eq!(
                launcher_command(Path::new(r"C:\Program Files\ErsatzRS\ErsatzRS.exe")),
                r#""C:\Program Files\ErsatzRS\ErsatzRS.exe""#
            );
        }

        #[test]
        fn registration_matching_is_case_insensitive_and_trimmed() {
            assert_eq!(
                classify_windows_startup_status(
                    Some(r#""C:\ErsatzRS\ErsatzRS.exe""#),
                    Some(r#" "C:\ERSATZRS\ERSATZRS.EXE" "#),
                ),
                WindowsStartupStatus::Enabled
            );
        }

        #[test]
        fn windows_startup_registry_round_trip_uses_an_isolated_subkey() {
            let current_user = RegKey::predef(HKEY_CURRENT_USER);
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after the Unix epoch")
                .as_nanos();
            let path = format!(r"Software\ErsatzRS\Tests\WindowsStartup-{nonce}");
            let (test_key, _) = current_user
                .create_subkey(&path)
                .expect("temporary registry key should be created");

            assert_eq!(
                read_registration_from(&test_key).expect("empty registration should be readable"),
                None
            );
            write_registration_to(&test_key, r#""C:\ErsatzRS\ErsatzRS.exe""#)
                .expect("registration should be written");
            assert_eq!(
                read_registration_from(&test_key).expect("registration should be readable"),
                Some(r#""C:\ErsatzRS\ErsatzRS.exe""#.to_owned())
            );
            delete_registration_from(&test_key).expect("registration should be deleted");
            assert_eq!(
                read_registration_from(&test_key).expect("deleted registration should be absent"),
                None
            );

            drop(test_key);
            current_user
                .delete_subkey_all(&path)
                .expect("temporary registry key should be removed");
        }
    }
}

#[cfg(windows)]
fn windows_startup_status() -> WindowsStartupStatus {
    windows_startup::status()
}

#[cfg(windows)]
fn set_windows_startup(enabled: bool) -> Result<(), SettingsRuntimeError> {
    windows_startup::set(enabled)
}

async fn validate_tool(path: &str, name: &str) -> Result<(), SettingsRuntimeError> {
    if path.trim().is_empty() {
        return Err(SettingsRuntimeError::unavailable(format!(
            "{name} path does not exist"
        )));
    }
    let output = tokio::process::Command::new(path)
        .arg("-version")
        .output()
        .await
        .map_err(|_| {
            SettingsRuntimeError::unavailable(format!("Unable to verify {name} version"))
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout.contains(&format!("{name} version")) {
        Ok(())
    } else {
        Err(SettingsRuntimeError::unavailable(format!(
            "Unable to verify {name} version"
        )))
    }
}

#[cfg(test)]
mod windows_startup_status_tests {
    use super::{classify_windows_startup_status, WindowsStartupStatus};

    #[test]
    fn missing_launcher_is_unavailable_even_when_registration_exists() {
        assert_eq!(
            classify_windows_startup_status(None, Some(r#""C:\Old\ErsatzRS\ErsatzRS.exe""#),),
            WindowsStartupStatus::Unavailable
        );
    }
}
