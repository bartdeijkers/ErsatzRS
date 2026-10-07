//! Explicit platform counterparts of the existing installed managed-FFmpeg producer fixture.
use super::*;

pub(super) async fn install(fixture: &Fixture, live: bool) {
    let package = fixture.root.path().join("package");
    tokio::fs::create_dir_all(&package).await.unwrap();
    let (name, kind, script) = entrypoint(live);
    tokio::fs::write(package.join(name), script).await.unwrap();
    let manifest = serde_json::json!({
        "manifest_version":1,"id":"org.ersatzrs.addon.row201","version":"1.0.0",
        "name":{"en-US":"Synthetic window producer"},"summary":{"en-US":"Disposable runtime proof"},
        "license":"GPL-3.0-or-later","source_url":"https://example.test/source","host_version":"*",
        "capabilities":[{"id":"addon.check.v1"},{"id":"remote-stream.play.v1"}],
        "entrypoints":{"any":{"path":name,"kind":kind}},
        "permissions":{"network_hosts":["example.test"],"media_read":false,"managed_ffmpeg":true,"external_commands":[]},
        "settings":[],"panels":[]
    });
    let hash = ersatzrs_infra::addons::calculate_addon_content_sha256(&package)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt") VALUES ('org.ersatzrs.addon.row201','1.0.0',1,?,?,?,?,?,?)"#)
        .bind("a".repeat(64)).bind(hash).bind(manifest.to_string()).bind(package.to_str().unwrap())
        .bind(Utc::now().to_rfc3339()).bind(Utc::now().to_rfc3339()).execute(&fixture.state.pool).await.unwrap();
    sqlx::query(r#"UPDATE "RemoteStream" SET "AddonId"='org.ersatzrs.addon.row201',"ProviderId"='item',"Url"='https://example.test/item' WHERE "Id"=10"#)
        .execute(&fixture.state.pool).await.unwrap();
}

#[cfg(not(windows))]
fn entrypoint(live: bool) -> (&'static str, &'static str, String) {
    let loop_args = if live { "-stream_loop -1" } else { "" };
    (
        "fixture.sh",
        "posix_shell",
        format!(
            r#"#!/bin/sh
set -eu
printf '%s\n' "$1" >> ../producer-starts
printf '%s\n' "$ERSATZRS_REMOTE_STREAM_SEEK" >> ../producer-seek
printf '%s\n' "$$" > ../producer-pid
exec "$FFMPEG_BIN" -v error -nostdin -re {loop_args} -ss "$ERSATZRS_REMOTE_STREAM_SEEK" -i ../fixture.mkv -map 0 -c copy -progress ../producer-progress -f matroska pipe:1
"#
        ),
    )
}

#[cfg(windows)]
fn entrypoint(live: bool) -> (&'static str, &'static str, String) {
    let loop_args = if live { "-stream_loop -1" } else { "" };
    (
        "fixture.cmd",
        "windows_batch",
        format!(
            r#"@echo off
>>..\producer-starts echo %1
>>..\producer-seek echo %ERSATZRS_REMOTE_STREAM_SEEK%
"%FFMPEG_BIN%" -v error -nostdin -re {loop_args} -ss "%ERSATZRS_REMOTE_STREAM_SEEK%" -i ..\fixture.mkv -map 0 -c copy -progress ..\producer-progress -f matroska pipe:1
exit /b %errorlevel%
"#
        ),
    )
}

pub(super) async fn assert_stopped(fixture: &Fixture, live: bool) {
    let starts = tokio::fs::read_to_string(fixture.root.path().join("producer-starts"))
        .await
        .unwrap();
    assert_eq!(
        starts.lines().count(),
        1,
        "one immutable occurrence must open its producer once"
    );
    let seek = tokio::fs::read_to_string(fixture.root.path().join("producer-seek"))
        .await
        .unwrap();
    assert_eq!(seek.trim(), if live { "0" } else { "00:00:03.0000000" });
    let progress = fixture.root.path().join("producer-progress");
    let before = tokio::fs::read(&progress).await.unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        before,
        tokio::fs::read(&progress).await.unwrap(),
        "a cleaned producer kept writing"
    );
    #[cfg(unix)]
    {
        let pid = tokio::fs::read_to_string(fixture.root.path().join("producer-pid"))
            .await
            .unwrap();
        let status = Command::new("kill")
            .args(["-0", pid.trim()])
            .status()
            .await
            .unwrap();
        assert!(
            !status.success(),
            "the owned producer survived confirmed cleanup"
        );
    }
    // Windows has the same confirmed session-transfer/managed-job cleanup assertion
    // in handoff(); the native driver additionally checks all owned fixture processes.
}
