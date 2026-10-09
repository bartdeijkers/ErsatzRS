//! Disposable installed producer and programme for the real router fixture.
use super::*;
use ersatzrs_core::repositories::{
    ChannelRepository as _, FfmpegProfileRepository as _, PlayoutRepository as _,
};
use ersatzrs_domain::{
    Channel, FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, PlayoutItem,
    StreamingEngine,
};

pub(super) async fn seed(pool: &ersatzrs_infra::db::DbPool, root: &Path) {
    install(pool, root).await;
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Synthetic',1,1);
        INSERT INTO "LocalLibrary" ("Id") VALUES (1);
        INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'media');
        INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (10,1,0);
        INSERT INTO "RemoteStream" ("Id","AddonId","ProviderId","Url","Liveness","IsLive") VALUES (10,'org.ersatzrs.addon.fixture','item','https://example.test/item',1,0);
        INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (30,10,0,'Synthetic','2026-09-23','2026-09-23');
        INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","RemoteStreamId","Height","Width","VideoScanKind") VALUES (100,'2026-09-23','2026-09-23','00:01:00',10,180,320,0);
        INSERT INTO "MediaStream" ("Id","Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId") VALUES
          (1,0,'h264',1,NULL,0,NULL,1,0,100),(2,1,'aac',2,'eng',1,'English',1,0,100),(3,2,'aac',2,'nld',1,'Dutch',0,0,100);
        INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (1,100,'unavailable/manifest.yml','fixture');
        INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Fixture',320,180);
    "#).execute(pool).await.unwrap();
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            resolution_id: 1,
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            video_bitrate: 1000,
            video_buffer_size: 2000,
            audio_sample_rate: 48,
            audio_channels: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    repos
        .channel
        .upsert(&Channel {
            id: 1,
            number: "1".into(),
            name: "Synthetic".into(),
            ffmpeg_profile_id: 1,
            streaming_engine: StreamingEngine::Next,
            preferred_audio_language_code: Some("nld".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (1,1,0,1)"#,
    )
    .execute(pool)
    .await
    .unwrap();
    let start = chrono::Utc::now() - chrono::Duration::seconds(2);
    repos
        .playout
        .upsert_item(&PlayoutItem {
            id: 1,
            playout_id: 1,
            media_item_id: 10,
            start,
            // Keep the finite source alive through the admitted 30s startup
            // window so these fixtures prove explicit Stop, rather than EOF.
            finish: start + chrono::Duration::seconds(54),
            in_point: chrono::Duration::seconds(6),
            out_point: chrono::Duration::seconds(60),
            ..Default::default()
        })
        .await
        .unwrap();
}

pub(crate) async fn install(pool: &ersatzrs_infra::db::DbPool, root: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let package = root.join("package");
    tokio::fs::create_dir_all(&package).await.unwrap();
    let script = package.join("fixture.sh");
    tokio::fs::write(&script, r#"#!/bin/sh
set -eu
printf '%s\n' "$1" >> ../producer-starts
printf '%s\n' "$$" > ../producer-pid
printf '%s\n' "$$" >> ../producer-pids
printf '%s\n' "$ERSATZRS_REMOTE_STREAM_SEEK" > ../producer-seek
exec "$FFMPEG_BIN" -v error -nostdin -re -ss "$ERSATZRS_REMOTE_STREAM_SEEK" -i ../source.mkv -map 0 -c copy -f matroska pipe:1
"#).await.unwrap();
    tokio::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .await
        .unwrap();
    let manifest = serde_json::json!({
        "manifest_version":1,"id":"org.ersatzrs.addon.fixture","version":"1.0.0",
        "name":{"en-US":"Fixture"},"summary":{"en-US":"Synthetic routed playback"},
        "license":"GPL-3.0-or-later","source_url":"https://example.test/source","host_version":"*",
        "capabilities":[{"id":"addon.check.v1"},{"id":"remote-stream.play.v1"}],
        "entrypoints":{"any":{"path":"fixture.sh","kind":"posix_shell"}},
        "permissions":{"network_hosts":["example.test"],"media_read":false,"managed_ffmpeg":true,"external_commands":[]},
        "settings":[],"panels":[]
    });
    let hash = ersatzrs_infra::addons::calculate_addon_content_sha256(&package)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt") VALUES ('org.ersatzrs.addon.fixture','1.0.0',1,?,?,?,?,?,?)"#)
        .bind("a".repeat(64)).bind(hash).bind(manifest.to_string()).bind(package.to_str().unwrap())
        .bind(chrono::Utc::now().to_rfc3339()).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await.unwrap();
}

pub(super) async fn generate(ffmpeg: &Path, root: &Path) {
    let mut cmd = Command::new(ffmpeg);
    cmd.args([
        "-v",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "color=blue:s=320x180:r=25",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=880:sample_rate=48000",
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-map",
        "2:a",
        "-t",
        "60",
        "-vf",
        "drawbox=color=red:t=fill:enable='lt(t,5)',drawbox=color=green:t=fill:enable='gte(t,12)'",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-g",
        "25",
        "-threads",
        "2",
        "-c:a",
        "aac",
        "-metadata:s:a:0",
        "language=eng",
        "-metadata:s:a:0",
        "title=English",
        "-metadata:s:a:1",
        "language=nld",
        "-metadata:s:a:1",
        "title=Dutch",
    ])
    .arg(root.join("source.mkv"));
    tool(cmd).await;
}
