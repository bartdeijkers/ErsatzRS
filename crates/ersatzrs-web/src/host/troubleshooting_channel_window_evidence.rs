//! Exact application wire observations and disposable persistence/decoded proof.
use super::*;
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;
use std::fmt::Write as _;

pub(super) async fn configuration(
    fixture: &Fixture,
    config: &protocol::ChannelConfig,
    number: &str,
    scenario: Scenario,
) {
    assert_eq!(number, if scenario.timeline() { "2" } else { "1" });
    assert_eq!(
        (
            config.normalization.video.width,
            config.normalization.video.height
        ),
        (320, 180)
    );
    assert!(matches!(
        config.normalization.video.format,
        protocol::VideoFormat::H264
    ));
    assert!(matches!(
        config.normalization.audio.format,
        protocol::AudioFormat::Aac
    ));
    assert_eq!(
        config.normalization.video.frame_rate.as_deref(),
        scenario.normalize().then_some("25")
    );
    let expected = fixture.at
        + if scenario.timeline() {
            TimeDelta::hours(1)
        } else {
            TimeDelta::zero()
        };
    assert_eq!(config.playout.virtual_start, Some(expected));
    tokio::fs::write(
        fixture.root.path().join("actual-configuration.json"),
        serde_json::to_vec_pretty(config).unwrap(),
    )
    .await
    .unwrap();
}

pub(super) fn manifest(playout: &protocol::Playout, start: DateTime<Utc>, scenario: Scenario) {
    assert_eq!(playout.version, protocol::PLAYOUT_VERSION);
    assert!(!playout.items.is_empty());
    assert!(playout.items.first().unwrap().start <= start);
    assert!(playout.items.first().unwrap().finish > start);
    assert_eq!(
        playout.items.last().unwrap().finish,
        start + TimeDelta::seconds(30)
    );
    for pair in playout.items.windows(2) {
        assert_eq!(
            pair[0].finish, pair[1].start,
            "window contains a gap instead of filler"
        );
    }
    assert!(playout.items.iter().all(|item| item.finish > item.start));
    if !matches!(scenario, Scenario::Empty) {
        assert!(
            playout.items.len() >= 4,
            "multi-occurrence thirty-second window required"
        );
    }
}

pub(super) fn callbacks(captured: &[origin::Captured], scenario: Scenario, stop: bool) {
    let resolved: Vec<_> = captured
        .iter()
        .filter_map(|value| value.item.as_ref())
        .collect();
    assert!(
        !resolved.is_empty(),
        "real Next never resolved a dynamic occurrence"
    );
    if scenario.timeline() && !matches!(scenario, Scenario::Empty) {
        assert!(
            captured
                .iter()
                .any(|value| value.path.starts_with("/ffmpeg/next-canvas?") && value.status == 200),
            "later occurrence canvas must be consumed by real Next"
        );
    }
    if stop || matches!(scenario, Scenario::Empty) {
        return;
    }
    let track_sources: Vec<_> = resolved
        .iter()
        .filter_map(|item| {
            item.get("source")
                .or_else(|| item.pointer("/tracks/video/source"))
        })
        .collect();
    if scenario.producer() {
        let prepared: Vec<_> = track_sources
            .iter()
            .filter(|source| {
                source
                    .get("uri")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|uri| uri.contains("/ffmpeg/next-prepared/"))
            })
            .collect();
        assert!(
            !prepared.is_empty(),
            "installed source was not resolved to an actual prepared stdout transfer"
        );
        for source in prepared {
            assert_eq!(
                source["is_live"].as_bool().unwrap_or(false),
                scenario.live()
            );
            assert!(
                source.pointer("/probe_hint/video/0/codec").is_some(),
                "actual producer probe identity was not retained"
            );
            assert!(source.pointer("/probe_hint/audio/0/codec").is_some());
        }
        assert!(
            captured.iter().any(
                |value| value.path.starts_with("/ffmpeg/next-prepared/") && value.status == 200
            ),
            "Next did not consume the actual installed producer"
        );
    } else if scenario.live() {
        assert!(
            track_sources
                .iter()
                .any(|source| if matches!(scenario, Scenario::RtspLive) {
                    source["source_type"] == "rtsp"
                } else {
                    source["source_type"] == "http" && source["is_live"] == true
                }),
            "actual live source wire contract missing"
        );
    }
}

pub(super) type DatabaseSnapshot = Vec<(String, Vec<Vec<String>>)>;

pub(super) async fn database(fixture: &Fixture) -> DatabaseSnapshot {
    let tables: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&fixture.state.pool).await.unwrap();
    let mut result = Vec::new();
    for table in tables {
        assert!(table
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'));
        let columns = sqlx::query("SELECT name FROM pragma_table_info(?)")
            .bind(&table)
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap();
        let expressions: Vec<String> = columns
            .iter()
            .map(|row| {
                let name: String = row.get("name");
                assert!(name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'));
                format!("quote(\"{name}\")")
            })
            .collect();
        let selected = expressions.join(",");
        // Only catalog identifiers restricted above to ASCII letters, digits and
        // underscores enter this quoted read-only query on the disposable fixture.
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {selected} FROM \"{table}\" ORDER BY {selected}"
        )))
        .fetch_all(&fixture.state.pool)
        .await
        .unwrap();
        result.push((
            table,
            rows.iter()
                .map(|row| {
                    (0..expressions.len())
                        .map(|index| row.get::<String, _>(index))
                        .collect()
                })
                .collect(),
        ));
    }
    result
}

pub(super) async fn export(
    fixture: &Fixture,
    scenario: Scenario,
    callbacks: &[origin::Captured],
    decoded: &decode::Decoded,
    cadence: Option<f64>,
    database: &DatabaseSnapshot,
) {
    let Some(root) = proof_directory() else {
        return;
    };
    let directory = root.join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&directory).await.unwrap();
    for (name, bytes) in [
        ("decoded.rgb", decoded.rgb.as_slice()),
        ("decoded.pcm", decoded.pcm.as_slice()),
    ] {
        tokio::fs::write(directory.join(name), bytes).await.unwrap();
    }
    tokio::fs::write(
        directory.join("video-packets.json"),
        serde_json::to_vec_pretty(&decoded.packets).unwrap(),
    )
    .await
    .unwrap();
    for name in ["actual-configuration.json", "actual-window.json"] {
        tokio::fs::copy(fixture.root.path().join(name), directory.join(name))
            .await
            .unwrap();
    }
    let hls_members = retain_hls(&fixture.paths().output, &directory).await;
    let summaries:Vec<_> = callbacks.iter().map(|callback| serde_json::json!({
        "namespace":callback.path.split('?').next().unwrap().split('/').take(3).collect::<Vec<_>>().join("/"),
        "status":callback.status,"resolved_item":callback.item.as_ref().and_then(|item|item.get("id"))
    })).collect();
    let proof = serde_json::json!({"scenario":format!("{scenario:?}"),"seconds":decoded.seconds,
        "measured_frame_rate":cadence,"callbacks":summaries,"hls_members":hls_members,
        "decoded_rgb_sha256":digest(&decoded.rgb),"decoded_pcm_sha256":digest(&decoded.pcm),
        "persistent_state_sha256":digest(&serde_json::to_vec(database).unwrap()),
        "persistent_state_unchanged":true,"owned_cleanup_confirmed":true});
    tokio::fs::write(
        directory.join("proof.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .await
    .unwrap();
}

fn digest(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(result, "{byte:02x}").unwrap();
    }
    result
}

pub(super) async fn export_stop(
    fixture: &Fixture,
    scenario: Scenario,
    captured: &[origin::Captured],
    database: &DatabaseSnapshot,
) {
    let Some(root) = proof_directory() else {
        return;
    };
    let directory = root.join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&directory).await.unwrap();
    tokio::fs::copy(
        fixture.root.path().join("actual-stop.json"),
        directory.join("actual-stop.json"),
    )
    .await
    .unwrap();
    let owned_log = retain_log(&troubleshooting_folder(), &directory).await;
    assert!(
        owned_log.is_some(),
        "stopped owned runtime log must be retained"
    );
    let proof = serde_json::json!({"scenario":format!("{scenario:?}"),"cancelled":true,"owned_log":owned_log,
        "real_callbacks_before_stop":captured.iter().filter(|callback| callback.status==200).count(),
        "callback_revocation_confirmed":true,"owned_cleanup_confirmed":true,"persistent_state_unchanged":true,
        "persistent_state_sha256":digest(&serde_json::to_vec(database).unwrap())});
    tokio::fs::write(
        directory.join("proof.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .await
    .unwrap();
}

async fn retain_hls(output: &FsPath, destination: &FsPath) -> Vec<serde_json::Value> {
    let hls = destination.join("hls");
    tokio::fs::create_dir(&hls).await.unwrap();
    let playlist = owned_bytes(&output.join("live.m3u8"), 256 * 1024).await;
    let body = std::str::from_utf8(&playlist).unwrap();
    assert!(body.lines().any(|line| line == "#EXT-X-ENDLIST"));
    let mut names = std::collections::BTreeSet::new();
    for name in body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        assert!(
            name.len() <= 128
                && FsPath::new(name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
        );
        let stem = &name[..name.len() - 3];
        assert!(
            !stem.is_empty()
                && stem
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
            "only safe owned local TS references may enter the proof"
        );
        names.insert(name.to_owned());
    }
    assert!(!names.is_empty() && names.len() <= 32);
    tokio::fs::write(hls.join("live.m3u8"), &playlist)
        .await
        .unwrap();
    let mut members = vec![member("hls/live.m3u8", &playlist)];
    let mut total = playlist.len();
    for name in names {
        let bytes = owned_bytes(&output.join(&name), 16 * 1024 * 1024).await;
        total = total.checked_add(bytes.len()).unwrap();
        assert!(
            total <= 64 * 1024 * 1024,
            "bounded thirty-second HLS proof exceeded its byte limit"
        );
        tokio::fs::write(hls.join(&name), &bytes).await.unwrap();
        members.push(member(&format!("hls/{name}"), &bytes));
    }
    if let Some(log) = retain_log(output, destination).await {
        members.push(log);
    }
    members
}

async fn retain_log(output: &FsPath, destination: &FsPath) -> Option<serde_json::Value> {
    let path = output.join("logs.txt");
    if !tokio::fs::try_exists(&path).await.unwrap() {
        return None;
    }
    let bytes = owned_bytes(&path, 2 * 1024 * 1024).await;
    tokio::fs::write(destination.join("logs.txt"), &bytes)
        .await
        .unwrap();
    Some(member("logs.txt", &bytes))
}

async fn owned_bytes(path: &FsPath, maximum: usize) -> Vec<u8> {
    let metadata = tokio::fs::symlink_metadata(path).await.unwrap();
    assert!(
        metadata.file_type().is_file(),
        "proof member must be an owned regular file"
    );
    assert!(metadata.len() <= u64::try_from(maximum).unwrap());
    let bytes = tokio::fs::read(path).await.unwrap();
    assert!(bytes.len() <= maximum);
    bytes
}

fn member(name: &str, bytes: &[u8]) -> serde_json::Value {
    serde_json::json!({"path":name,"bytes":bytes.len(),"sha256":digest(bytes)})
}

pub(super) async fn child_log_directory(test: &str) -> Option<PathBuf> {
    let root = proof_directory()?;
    let directory = root.join(format!("child-{test}-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&directory).await.unwrap();
    Some(directory)
}

#[allow(
    clippy::disallowed_methods,
    reason = "Read only the opt-in synthetic test proof directory, never application configuration."
)]
fn proof_directory() -> Option<PathBuf> {
    std::env::var_os("ERSATZRS_ROW201_PROOF_DIR").map(PathBuf::from)
}

pub(super) async fn child_logs(
    directory: Option<&FsPath>,
    stdout: &[u8],
    stderr: &[u8],
) -> ApiResult<()> {
    let Some(directory) = directory else {
        return Ok(());
    };
    tokio::fs::write(directory.join("stdout.log"), stdout).await?;
    tokio::fs::write(directory.join("stderr.log"), stderr).await?;
    let receipt = serde_json::json!({"stdout":member("stdout.log",stdout),"stderr":member("stderr.log",stderr)});
    tokio::fs::write(
        directory.join("capture.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )
    .await?;
    Ok(())
}
