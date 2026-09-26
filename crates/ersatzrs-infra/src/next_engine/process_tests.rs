use super::*;
use ersatzrs_domain::{
    FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, Resolution,
};

const HELPER_MODE: &str = "ERSATZRS_NEXT_PROCESS_TEST_MODE";
const HELPER_FOLDER: &str = "ERSATZRS_NEXT_PROCESS_TEST_FOLDER";

pub(super) fn config() -> ChannelConfig {
    super::super::config::channel_config(
        &crate::playback::PlaybackProfile {
            profile: FFmpegProfile {
                audio_format: FFmpegProfileAudioFormat::Aac,
                video_format: FFmpegProfileVideoFormat::H264,
                ..Default::default()
            },
            resolution: Resolution {
                width: 320,
                height: 180,
                ..Default::default()
            },
        },
        ersatzrs_domain::NextEngineTextSubtitleMode::Burn,
        Path::new("playouts/current"),
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
    )
    .unwrap()
}

pub(super) fn fixture_command(mode: &str, folder: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "next_engine::process::tests::fixture_child",
            "--ignored",
            "--nocapture",
        ])
        .env(HELPER_MODE, mode)
        .env(HELPER_FOLDER, folder);
    command
}

// A fresh instance of the test executable is a portable process fixture. It
// avoids shell dependencies and uses the real process-group/job implementation.
#[test]
#[ignore = "invoked only by the managed Next process tests"]
#[allow(clippy::disallowed_methods)] // Test-only environment selects isolated child behavior.
fn fixture_child() {
    use std::io::Read;
    let Ok(mode) = std::env::var(HELPER_MODE) else {
        return;
    };
    let folder = PathBuf::from(std::env::var_os(HELPER_FOLDER).unwrap());
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes).unwrap();
    // Emitting readiness requires complete stdin delivery, including EOF.
    let _: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    std::fs::write(folder.join("received.json"), bytes).unwrap();
    if mode == "fail" {
        std::process::exit(7);
    }
    if mode == "troubleshoot" {
        std::fs::write(folder.join("ffmpeg.m3u8"), b"#EXTM3U\n#EXT-X-ENDLIST\n").unwrap();
        std::fs::write(folder.join("ffreport.log"), b"completed fixture").unwrap();
        return;
    }
    if mode == "ready" || mode == "ready-exit" {
        std::fs::write(folder.join(".ready"), b"").unwrap();
    }
    loop {
        if mode == "ready-exit" && folder.join("exit-request").exists() {
            std::process::exit(9);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[tokio::test]
async fn readiness_requires_complete_stdin_and_stop_reaps_the_managed_child() {
    let folder = tempfile::tempdir().unwrap();
    let config = config();
    let mut process = NextProcess::start_command(
        fixture_command("ready", folder.path()),
        folder.path(),
        &config,
        Instant::now() + Duration::from_secs(10),
    )
    .await
    .unwrap();
    let received: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.path().join("received.json")).unwrap())
            .unwrap();
    assert_eq!(received, serde_json::to_value(config).unwrap());
    process.touch().await.unwrap();
    assert!(folder.path().join(".heartbeat").is_file());
    process.stop().await.unwrap();
    assert!(!process.wait().await.unwrap().success());
}

#[tokio::test]
async fn stale_readiness_cannot_acknowledge_a_hung_new_process() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join(".ready"), b"old session").unwrap();
    let result = NextProcess::start_command(
        fixture_command("wait", folder.path()),
        folder.path(),
        &config(),
        Instant::now() + Duration::from_millis(500),
    )
    .await;
    assert!(matches!(result, Err(ProcessError::StartupTimeout)));
    assert!(!folder.path().join(".ready").exists());
}

#[tokio::test]
async fn early_exit_is_reported_without_waiting_for_the_startup_deadline() {
    let folder = tempfile::tempdir().unwrap();
    let result = NextProcess::start_command(
        fixture_command("fail", folder.path()),
        folder.path(),
        &config(),
        Instant::now() + Duration::from_secs(10),
    )
    .await;
    assert!(matches!(
        result,
        Err(ProcessError::ExitedBeforeReady(Some(7)))
    ));
}

#[tokio::test]
async fn finite_troubleshooting_delivers_stdin_and_preserves_exit_and_artifacts_without_readiness()
{
    for (mode, expected_code) in [("troubleshoot", 0), ("fail", 7)] {
        let folder = tempfile::tempdir().unwrap();
        let config = config();
        let mut process = NextProcess::start_configured(
            fixture_command(mode, folder.path()),
            folder.path(),
            &config,
            Instant::now() + Duration::from_secs(10),
            StartupMode::Troubleshooting,
        )
        .await
        .unwrap();
        let status = timeout(Duration::from_secs(10), process.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.code(), Some(expected_code));
        assert!(!folder.path().join(".ready").exists());
        let received: serde_json::Value = serde_json::from_slice(
            &tokio::fs::read(folder.path().join("received.json"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(received, serde_json::to_value(config).unwrap());
        if status.success() {
            assert!(folder.path().join("ffmpeg.m3u8").is_file());
            assert!(folder.path().join("ffreport.log").is_file());
        }
    }
}

#[tokio::test]
async fn troubleshooting_uses_only_default_overlay_and_explicit_run_mode() {
    let root = tempfile::tempdir().unwrap();
    let default = root.path().join("default.json");
    tokio::fs::write(&default, "{}").await.unwrap();
    tokio::fs::write(root.path().join("1.json"), "{}")
        .await
        .unwrap();
    let command = troubleshooting_command(
        Path::new("fixture-next"),
        "1",
        Path::new("fixture-output"),
        Some(root.path()),
        Instant::now() + Duration::from_secs(10),
    )
    .await
    .unwrap();
    let args: Vec<_> = command
        .as_std()
        .get_args()
        .map(std::ffi::OsStr::to_os_string)
        .collect();
    assert_eq!(
        args,
        [
            "run".into(),
            "--output-folder".into(),
            "fixture-output".into(),
            "--number=1".into(),
            "-".into(),
            "--troubleshoot".into(),
            default.into_os_string(),
        ]
    );
}

#[tokio::test]
async fn expired_deadline_and_missing_binary_return_redacted_errors() {
    let folder = tempfile::tempdir().unwrap();
    let missing = folder.path().join("private-missing-executable");
    let expired = NextProcess::start(&missing, "1", folder.path(), &config(), Instant::now()).await;
    assert!(matches!(expired, Err(ProcessError::StartupTimeout)));
    let error = NextProcess::start(
        &missing,
        "1",
        folder.path(),
        &config(),
        Instant::now() + Duration::from_secs(10),
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(
        error,
        ProcessError::Io {
            operation: "spawn",
            ..
        }
    ));
    assert!(!error.to_string().contains("private-missing-executable"));
    assert!(!error
        .to_string()
        .contains(&folder.path().display().to_string()));
}

#[test]
fn session_identity_is_reserved_to_the_child_and_never_inherited() {
    let mut command = Command::new("fixture-next");
    command.env(super::super::media::SESSION_ENV, "stale-fixture-value");
    let instance = uuid::Uuid::new_v4();
    set_session_environment(&mut command, Some(instance));
    let lookup = |command: &Command| {
        command
            .as_std()
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new(super::super::media::SESSION_ENV))
            .map(|(_, value)| value.map(std::ffi::OsStr::to_os_string))
    };
    assert_eq!(lookup(&command), Some(Some(instance.to_string().into())));
    set_session_environment(&mut command, None);
    assert_eq!(lookup(&command), Some(None));
}
