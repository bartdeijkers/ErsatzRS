use super::*;
use std::ffi::OsString;

async fn arguments(directory: Option<&Path>, number: &str) -> Result<Vec<OsString>, ProcessError> {
    let command = channel_command(
        Path::new("next-engine"),
        number,
        Path::new("output"),
        directory,
        Instant::now() + Duration::from_secs(10),
    )
    .await?;
    Ok(command.as_std().get_args().map(OsString::from).collect())
}

fn base_arguments() -> Vec<OsString> {
    ["run", "--output-folder", "output", "--number=12.3", "-"]
        .into_iter()
        .map(OsString::from)
        .collect()
}

#[tokio::test]
async fn overlays_follow_stdin_in_default_then_channel_precedence() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("overlay files with spaces");
    std::fs::create_dir(&directory).unwrap();
    let default = directory.join("default.json");
    let channel = directory.join("12.3.json");
    std::fs::write(&default, b"{}").unwrap();
    std::fs::write(&channel, b"{}").unwrap();
    std::fs::write(directory.join("99.json"), b"{}").unwrap();
    let mut expected = base_arguments();
    expected.push(default.into_os_string());
    expected.push(channel.into_os_string());
    assert_eq!(arguments(Some(&directory), "12.3").await.unwrap(), expected);
}

#[tokio::test]
async fn either_overlay_can_exist_independently_and_missing_directory_is_optional() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(arguments(None, "12.3").await.unwrap(), base_arguments());
    assert_eq!(
        arguments(Some(&directory.path().join("absent")), "12.3")
            .await
            .unwrap(),
        base_arguments()
    );
    for filename in ["default.json", "12.3.json"] {
        let path = directory.path().join(filename);
        std::fs::write(&path, b"{}").unwrap();
        let mut expected = base_arguments();
        expected.push(path.as_os_str().to_owned());
        assert_eq!(
            arguments(Some(directory.path()), "12.3").await.unwrap(),
            expected
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[tokio::test]
async fn unsafe_channel_filenames_are_rejected_on_every_platform() {
    let directory = tempfile::tempdir().unwrap();
    for number in [
        "", ".", "..", "../1", "1/2", r"1\2", r"C:\1", "1:2", "1\0", "1.2.3", "-1", "1.",
    ] {
        let error = arguments(Some(directory.path()), number).await.unwrap_err();
        assert!(matches!(error, ProcessError::Preparation(_)));
        assert_eq!(
            error.to_string(),
            "Next startup preparation failed: invalid overlay channel number"
        );
    }
}

#[tokio::test]
async fn non_file_overlay_is_an_error_without_private_path_disclosure() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("default.json")).unwrap();
    let error = arguments(Some(directory.path()), "12.3").await.unwrap_err();
    assert!(matches!(
        error,
        ProcessError::Io {
            kind: io::ErrorKind::InvalidData,
            ..
        }
    ));
    assert!(!error
        .to_string()
        .contains(&directory.path().display().to_string()));
}

#[tokio::test]
async fn overlay_discovery_respects_an_expired_startup_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let result = channel_command(
        Path::new("unused"),
        "12.3",
        Path::new("output"),
        Some(directory.path()),
        Instant::now(),
    )
    .await;
    assert!(matches!(result, Err(ProcessError::StartupTimeout)));
}

// Unix shell fixture exercises the public spawn API. Platform-independent argv
// tests above cover the same ordering on Windows without requiring a shell.
#[cfg(unix)]
#[tokio::test]
async fn spawned_engine_receives_overlay_paths_after_stdin_and_failed_startup_is_reaped() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let overlays = root.path().join("overlay files");
    std::fs::create_dir(&overlays).unwrap();
    std::fs::write(overlays.join("default.json"), b"{}").unwrap();
    std::fs::write(overlays.join("12.3.json"), b"malformed JSON").unwrap();
    let executable = root.path().join("fixture-engine");
    std::fs::write(
        &executable,
        b"#!/bin/sh\noutput=$3\nprintf '%s\\n' \"$@\" > \"$output/arguments\"\ncat > \"$output/config.json\"\nexit 7\n",
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = root.path().join("output");
    let result = NextProcess::start_with_overlays(
        &executable,
        "12.3",
        &output,
        &super::tests::config(),
        Some(&overlays),
        Instant::now() + Duration::from_secs(10),
    )
    .await;
    assert!(matches!(
        result,
        Err(ProcessError::ExitedBeforeReady(Some(7)))
    ));
    let arguments = std::fs::read_to_string(output.join("arguments")).unwrap();
    let expected = [
        "run".to_owned(),
        "--output-folder".to_owned(),
        output.display().to_string(),
        "--number=12.3".to_owned(),
        "-".to_owned(),
        overlays.join("default.json").display().to_string(),
        overlays.join("12.3.json").display().to_string(),
    ];
    assert_eq!(arguments.lines().collect::<Vec<_>>(), expected);
    let received: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("config.json")).unwrap()).unwrap();
    assert_eq!(
        received,
        serde_json::to_value(super::tests::config()).unwrap()
    );
    assert!(!output.join(".ready").exists());
}

async fn preview_arguments(
    directory: Option<&Path>,
    number: &str,
) -> Result<Vec<OsString>, ProcessError> {
    let command = troubleshooting_command(
        Path::new("next-engine"),
        number,
        Path::new("output"),
        directory,
        Instant::now() + Duration::from_secs(10),
    )
    .await?;
    Ok(command.as_std().get_args().map(OsString::from).collect())
}

#[tokio::test]
async fn channel_troubleshooting_keeps_default_then_channel_overlay_precedence() {
    let directory = tempfile::tempdir().unwrap();
    let default = directory.path().join("default.json");
    let channel = directory.path().join("12.3.json");
    let mut base = base_arguments();
    base.push("--troubleshoot".into());
    assert_eq!(
        preview_arguments(Some(&directory.path().join("absent")), "12.3")
            .await
            .unwrap(),
        base
    );
    for existing in [vec![&default], vec![&channel], vec![&default, &channel]] {
        for path in &existing {
            tokio::fs::write(path, b"{}").await.unwrap();
        }
        let mut expected = base.clone();
        expected.extend(existing.iter().map(|path| path.as_os_str().to_owned()));
        assert_eq!(
            preview_arguments(Some(directory.path()), "12.3")
                .await
                .unwrap(),
            expected
        );
        for path in existing {
            tokio::fs::remove_file(path).await.unwrap();
        }
    }
}

#[tokio::test]
async fn channel_troubleshooting_rejects_unsafe_names_without_touching_other_overlays() {
    let directory = tempfile::tempdir().unwrap();
    for number in ["", "../1", "1/2", r"1\2", "1.2.3", "1.", ".other", "-1"] {
        assert!(matches!(
            preview_arguments(Some(directory.path()), number).await,
            Err(ProcessError::Preparation(_))
        ));
    }
    tokio::fs::write(directory.path().join("default.json"), b"{}")
        .await
        .unwrap();
    tokio::fs::create_dir(directory.path().join(".troubleshooting.json"))
        .await
        .unwrap();
    let actual = preview_arguments(Some(directory.path()), ".troubleshooting")
        .await
        .unwrap();
    assert_eq!(
        actual.last().unwrap(),
        directory.path().join("default.json").as_os_str()
    );
}
