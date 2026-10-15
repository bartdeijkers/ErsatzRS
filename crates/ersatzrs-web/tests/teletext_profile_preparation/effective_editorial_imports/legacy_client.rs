//! Owns genuine cumulative scoped designs while an in-crate actor serves them.
//! The closed handoff grants no production host API or publication authority.

use super::super::{
    client_capture,
    editorial_imports::live_client::{hash, read_bounded},
    ordinary_stream_flow,
};
use super::live_client::{
    channel_facts, complete_with_zone, configure_source, fixture, recover, retained_state,
    same_output, write_new, Flow,
};
use serde_json::{json, Value};
use std::{
    io,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::time::Instant;

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_SCOPED_LEGACY_CLIENT_DIR";
const SOURCE_MAXIMUM: usize = 48 * 1024 * 1024;

fn template(flow: &Flow, source: &[u8]) -> Value {
    let mut channels = [channel_facts(flow, 0), channel_facts(flow, 1)];
    for channel in &mut channels {
        channel["url"] = Value::Null;
    }
    json!({"probe_version":3,
        "scope":"Cumulative effective-role concepts published/reopened at cold actual generated legacy URLs; no replay",
        "source":{"file":"source.ts","sha256":hash(source),"bytes":source.len(),"duration_seconds":60},
        "channels":channels,"active_transcode_limit":4,
        "completion_file":"client-complete.json","completion_timeout_seconds":180,
        "expected_completion":{"probe_version":3,"client_name":"VLC","client_version":"3.0.20",
            "decoder_version":"0.2.42","proof_suffix":"cumulative-effective-legacy-ordinary","termination":"owned-sigterm"},
        "client_actions":"Open both actual legacy URLs; navigate 200 to 201 to 200, verify day-only Q/R, then stop the verified owned clients with SIGTERM before root completion.",
        "limitations":"Representative cold legacy proof only. Later prefix responses are independent cold reconnects. No natural EOF, programme handoff, warm cache, full Level 2.5 or platform acceptance."})
}

fn closed_keys(value: &Value, keys: &[&str]) -> io::Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| io::Error::other("handoff is not an object"))?;
    if object.len() != keys.len() || keys.iter().any(|key| !object.contains_key(*key)) {
        return Err(io::Error::other("handoff has missing or unknown fields"));
    }
    Ok(())
}

async fn wait_json(directory: &Path, name: &str, deadline: Instant) -> io::Result<Value> {
    tokio::time::timeout_at(deadline, async {
        loop {
            let path = directory.join(name);
            if tokio::fs::try_exists(&path).await? {
                return serde_json::from_slice(&read_bounded(&path, 65536)?)
                    .map_err(|_| io::Error::other("invalid closed handoff JSON"));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("owned legacy handoff exceeded its deadline"))?
}

fn origin(value: &Value, nonce: &str) -> io::Result<std::net::SocketAddr> {
    closed_keys(
        value,
        &["version", "nonce", "public_address", "internal_address"],
    )?;
    if value["version"] != 1 || value["nonce"] != nonce {
        return Err(io::Error::other("actor origin identity differs"));
    }
    let parse = |key: &str| {
        value[key]
            .as_str()
            .and_then(|text| text.parse::<std::net::SocketAddr>().ok())
            .filter(|address| {
                address.ip() == std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                    && address.port() != 0
            })
            .ok_or_else(|| io::Error::other("actor origin is not owned IPv4 loopback"))
    };
    let public = parse("public_address")?;
    if public == parse("internal_address")? {
        return Err(io::Error::other("actor listeners overlap"));
    }
    Ok(public)
}

async fn assert_unchanged(flow: &Flow, baseline: &[Vec<String>], calls: &[u8], package: &str) {
    let fixture = &flow.fixture.fixture;
    for (index, expected) in flow.accepted.iter().enumerate() {
        same_output(
            &recover(fixture, i64::try_from(index + 1).unwrap()).await,
            expected,
        );
    }
    assert_eq!(retained_state(fixture).await, baseline);
    assert_eq!(flow.fixture.calls(), calls);
    assert_eq!(
        ersatzrs_infra::addons::calculate_addon_content_sha256(
            &fixture.root.path().join("package")
        )
        .await
        .unwrap(),
        package
    );
}

async fn run() {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    std::fs::create_dir(&directory).unwrap();
    let directory = directory.canonicalize().unwrap();
    let tools = ordinary_stream_flow::Tools::load().await;
    let fixture = fixture().await;
    let source_path = client_capture::generate_source_with_duration(
        &tools.ffmpeg,
        fixture.fixture.root.path(),
        60,
    )
    .await;
    assert_eq!(
        source_path.file_name().unwrap(),
        "synthetic-client-source.ts"
    );
    configure_source(&fixture.fixture, &source_path).await;
    ordinary_stream_flow::setting(
        &fixture.fixture,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT,
        "",
    )
    .await;
    let source = read_bounded(&source_path, SOURCE_MAXIMUM).unwrap();
    assert!(!source.is_empty() && source.len().is_multiple_of(188));
    write_new(&directory.join("source.ts"), &source).unwrap();
    let flow = Box::pin(complete_with_zone(fixture, true)).await;
    let baseline = retained_state(&flow.fixture.fixture).await;
    let calls = flow.fixture.calls();
    let package = ersatzrs_infra::addons::calculate_addon_content_sha256(
        &flow.fixture.fixture.root.path().join("package"),
    )
    .await
    .unwrap();
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let marker = json!({"version":1,"nonce":nonce,"proof_directory":directory});
    write_new(
        &flow.fixture.fixture.root.path().join("legacy-owner.json"),
        &serde_json::to_vec(&marker).unwrap(),
    )
    .unwrap();
    let handoff = json!({"version":1,"nonce":nonce,"owner_root":flow.fixture.fixture.root.path().canonicalize().unwrap(),
        "database":"designs.sqlite","source":"synthetic-client-source.ts","source_sha256":hash(&source),
        "source_bytes":source.len(),"package_sha256":package,"ready_template":template(&flow,&source)});
    write_new(
        &directory.join("owner-handoff.json"),
        &serde_json::to_vec_pretty(&handoff).unwrap(),
    )
    .unwrap();
    let result = owner_wait(&directory, &nonce, &handoff).await;
    if result.is_ok() {
        assert_unchanged(&flow, &baseline, &calls, &package).await;
        assert_eq!(read_bounded(&source_path, SOURCE_MAXIMUM).unwrap(), source);
    }
    flow.fixture.fixture.state.pool.close().await;
    let done = result.expect("prepared legacy owner did not receive verified actor completion");
    write_new(&directory.join("success.json"),&serde_json::to_vec_pretty(&json!({"probe_version":3,
        "actor":done,"roles_roots_scope_packages_profiles_settings_calls_generations_unchanged":true,
        "evidence_scope":"Headed receipts require root inspection; actor independently verifies new cold prefixes and cleanup. No original-response transport, natural EOF or warm continuity claim."})).unwrap()).unwrap();
}

async fn owner_wait(directory: &Path, nonce: &str, handoff: &Value) -> io::Result<Value> {
    let actor = wait_json(
        directory,
        "actor-ready.json",
        Instant::now() + Duration::from_secs(30),
    )
    .await?;
    let address = origin(&actor, nonce)?;
    let mut ready = handoff["ready_template"].clone();
    for channel in ready["channels"].as_array_mut().unwrap() {
        channel["url"] = json!(format!(
            "http://{address}/iptv/channel/{}.ts?mode=ts-legacy",
            channel["channel_id"].as_i64().unwrap()
        ));
    }
    let ready = serde_json::to_vec_pretty(&ready).unwrap();
    write_new(&directory.join("ready.json"), &ready)?;
    let ready_hash = hash(&ready);
    write_new(
        &directory.join("owner-ready.json"),
        &serde_json::to_vec(&json!({"version":1,"nonce":nonce,"ready_sha256":ready_hash})).unwrap(),
    )?;
    let done = wait_json(
        directory,
        "actor-done.json",
        Instant::now() + Duration::from_secs(225),
    )
    .await?;
    closed_keys(
        &done,
        &[
            "version",
            "nonce",
            "ready_sha256",
            "source_sha256",
            "capture_sha256",
            "capture_bytes",
            "capacity_active",
            "public_registry_empty",
            "listeners_joined",
        ],
    )?;
    if done["version"] != 1
        || done["nonce"] != nonce
        || done["ready_sha256"] != ready_hash
        || done["source_sha256"] != handoff["source_sha256"]
        || done["capacity_active"] != 0
        || done["public_registry_empty"] != true
        || done["listeners_joined"] != true
    {
        return Err(io::Error::other(
            "actor completion differs from this prepared owner",
        ));
    }
    if done["capture_sha256"]
        .as_array()
        .is_none_or(|array| array.len() != 2)
        || done["capture_bytes"]
            .as_array()
            .is_none_or(|array| array.len() != 2)
    {
        return Err(io::Error::other("actor capture identity count differs"));
    }
    for (index, name) in ["day.cold-prefix.ts", "night.cold-prefix.ts"]
        .into_iter()
        .enumerate()
    {
        let bytes = read_bounded(&directory.join(name), 8 * 1024 * 1024)?;
        if bytes.len() < 2 * 1024 * 1024
            || !bytes.len().is_multiple_of(188)
            || done["capture_sha256"][index] != hash(&bytes)
            || done["capture_bytes"][index] != bytes.len()
        {
            return Err(io::Error::other("actor cold prefix association differs"));
        }
    }
    Ok(done)
}

#[tokio::test]
#[ignore = "Requires separate prebuilt in-crate legacy actor, pinned native tools and two owned headed clients; genuine preparation owner remains alive"]
async fn cumulative_scoped_originals_serve_generated_legacy_urls_for_two_headed_clients() {
    Box::pin(run()).await;
}
