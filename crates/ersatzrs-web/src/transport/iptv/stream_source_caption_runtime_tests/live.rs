//! Ordinary public live add-on route: one producer, source captions and audio.
//! The shell fixture is intentionally Unix-only.
#![cfg(unix)]

use super::*;

const PRODUCER: &str = "org.ersatzrs.remote.live-caption-fixture";

async fn install_producer(state: &AppState, root: &Path, item: i64, version: i64) {
    let package = root.join("live-package");
    tokio::fs::create_dir(&package).await.unwrap();
    tokio::fs::write(
        package.join("fixture.sh"),
        br#"set -eu
case "$1" in
check) printf '%s' '{"status":"ready","code":"ready","message":"ok"}'; exit 0;;
play) ;;
*) exit 9;;
esac
printf 'play\n' >> ../producer-starts
printf '%s\n' "$$" > ../producer.pid
printf '%s\n' "$ERSATZRS_REMOTE_STREAM_SEEK" >> ../producer-seeks
cat '../library/Synthetic Caption.ts'
printf 'sent\n' > ../producer-source-sent
while :; do sleep 1; done
"#,
    )
    .await
    .unwrap();
    let manifest: AddonManifest = serde_json::from_value(json!({
        "manifest_version":ersatzrs_addon_contract::ADDON_MANIFEST_VERSION,
        "id":PRODUCER,"version":"1.0.0","name":{"en-US":"Synthetic live captions"},
        "summary":{"en-US":"Single producer ordinary native route proof"},
        "license":"GPL-3.0-or-later","source_url":"https://example.test/source","host_version":"*",
        "capabilities":[{"id":"addon.check.v1","options":[]},{"id":"remote-stream.play.v1","options":[]}],
        "entrypoints":{"any":{"path":"fixture.sh","kind":"posix_shell"}}
    })).unwrap();
    manifest.validate().unwrap();
    let hash = ersatzrs_infra::addons::calculate_addon_content_sha256(&package)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt","StateRevision")
        VALUES (?,'1.0.0',0,?,?,?,?,'now','now','before-enable')"#)
        .bind(PRODUCER).bind("b".repeat(64)).bind(hash)
        .bind(serde_json::to_string(&manifest).unwrap()).bind(package.to_str().unwrap())
        .execute(&state.pool).await.unwrap();
    state
        .application
        .enable_addon(PRODUCER.into())
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "RemoteStream" ("Id","AddonId","ProviderId","Url","Liveness","IsLive")
        VALUES (?,?,'item','https://example.test/live-caption',2,1)"#,
    )
    .bind(item)
    .bind(PRODUCER)
    .execute(&state.pool)
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=? WHERE "Id"=?"#)
        .bind(item)
        .bind(version)
        .execute(&state.pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "MovieMetadata" WHERE "MovieId"=?"#)
        .bind(item)
        .execute(&state.pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "Movie" WHERE "Id"=?"#)
        .bind(item)
        .execute(&state.pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "RemoteStreamMetadata" ("RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated")
        VALUES (?,0,'Synthetic live captions','2026-10-22','2026-10-22')"#)
        .bind(item).execute(&state.pool).await.unwrap();
    // No playable local path can rescue the route by reopening the fixture.
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"='unavailable/live-definition.ts' WHERE "MediaVersionId"=?"#)
        .bind(version).execute(&state.pool).await.unwrap();
}

async fn assert_producer_gone(root: &Path) {
    let pid = tokio::fs::read_to_string(root.join("producer.pid"))
        .await
        .unwrap();
    assert!(pid.trim().parse::<u32>().unwrap() > 0);
    let mut command = tokio::process::Command::new("sh");
    command.args([
        "-c",
        "kill -0 \"$1\" 2>/dev/null",
        "synthetic-pid-check",
        pid.trim(),
    ]);
    command.kill_on_drop(true);
    assert!(
        !tokio::time::timeout(Duration::from_secs(2), command.status())
            .await
            .unwrap()
            .unwrap()
            .success(),
        "actual producer must be joined after response drop"
    );
}

async fn run() {
    let tools = initialize_source_caption_runtime().await;
    let root = tempfile::tempdir().unwrap().keep();
    println!("ordinary live source-caption evidence: {}", root.display());
    let library = root.join("library");
    let source = local_source(&tools[0], &library).await;
    let expected = calibration(&tools, &source, &root).await;
    let unused_provider = Server::start(Router::new()).await;
    let state = application(&root, &unused_provider).await;
    let (item, version) = scan_source(&state, &library, &tools[1]).await;
    software_profile(&state).await;
    install_producer(&state, &root, item, version).await;
    let generation = install_and_accept(&state, &root).await;
    let internal = Server::start(build_internal_app(state.clone())).await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    let calls = tokio::fs::read(root.join("addon-calls")).await.unwrap();
    for (index, mode) in [ChannelSubtitleMode::None, ChannelSubtitleMode::Any]
        .into_iter()
        .enumerate()
    {
        reset_active_transcode_state_for_tests();
        start_item(&state, item, mode, "nld", StreamingMode::TransportStream).await;
        let label = if mode == ChannelSubtitleMode::None {
            "off"
        } else {
            "burn"
        };
        let output =
            capture_recorded(&public, Some(&root.join(format!("{label}-partial.ts")))).await;
        tokio::fs::write(root.join(format!("{label}.ts")), &output)
            .await
            .unwrap();
        assert_producer_gone(&root).await;
        let starts = tokio::fs::read_to_string(root.join("producer-starts"))
            .await
            .unwrap();
        assert_eq!(
            starts.lines().count(),
            index + 1,
            "one source producer per HTTP request"
        );
        let seeks = tokio::fs::read_to_string(root.join("producer-seeks"))
            .await
            .unwrap();
        assert!(
            seeks
                .lines()
                .all(|line| line.parse::<f64>().unwrap() == 0.0),
            "live source retains its existing zero-seek policy"
        );
        let actual = pixels(&tools[0], &output, None).await;
        verify_case(
            &tools,
            &output,
            &expected,
            &actual,
            mode != ChannelSubtitleMode::None,
            "nld",
        )
        .await;
        assert_eq!(
            state
                .application
                .operations()
                .entity_locker()
                .active_teletext_work(),
            0
        );
        assert_eq!(
            tokio::fs::read(root.join("addon-calls")).await.unwrap(),
            calls
        );
        assert_eq!(
            state
                .application
                .accepted_teletext_generation(1)
                .await
                .unwrap()
                .unwrap()
                .id,
            generation
        );
        println!("ordinary live source captions verified: {label}, single producer, Dutch660Hz, defaultcapacity2, joined cleanup");
    }
    finish_caption_owner(
        &state,
        None,
        MediaSourceKind::Local,
        public,
        internal,
        unused_provider,
    )
    .await;
}

#[test]
#[ignore = "requires pinned FFmpeg/ffprobe9+libzvbi/libx264, TSDuck and loopback; installed Unix live producer and public native route"]
fn normal_live_addon_route_preserves_single_producer_captions_off_and_alternate_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run()));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}
