//! Private cold legacy actor for a bounded genuine fixture handoff. The owner
//! process retains database/package lifetime; no public host API is widened.

use super::super::teletext_retained_output::runtime_tests::{decode_text, exact_tool};
use super::ordinary_wrapper_tests as checks;
use crate::host::{
    router::{build_app, build_internal_app},
    state::{AppState, IptvAccessTokenVerifier, SystemStartup},
    teletext_effective_generations::reload_accepted_effective_teletext_generation,
    teletext_runtime_policy::parse_native_teletext_runtime_policy,
};
use axum::{
    body::Body,
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
    Router,
};
use chrono::{DateTime, Utc};
use ersatzrs_infra::{
    db::{DatabaseBackend, DatabaseSchemaStatus},
    transcode_capacity::{
        active_transcode_snapshot, active_transcode_test_guard_for_tests,
        reset_active_transcode_state_for_tests,
    },
};
use futures_util::{FutureExt as _, StreamExt as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeSet,
    io,
    net::SocketAddr,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle, time::Instant};
use tokio_util::sync::CancellationToken;

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_SCOPED_LEGACY_CLIENT_DIR";
const MAX_SOURCE: usize = 48 * 1024 * 1024;
const MAX_PREFIX: usize = 8 * 1024 * 1024;
const ADDON: &str = "org.ersatzrs.teletext.native-profile-fixture";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Handoff {
    version: u8,
    nonce: String,
    owner_root: PathBuf,
    database: String,
    source: String,
    source_sha256: String,
    source_bytes: usize,
    package_sha256: String,
    ready_template: Value,
}

fn hash(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(value, "{byte:02x}").unwrap();
    }
    value
}
fn invalid() -> io::Error {
    io::Error::other("invalid owned scoped legacy handoff")
}

fn read(path: &Path, maximum: usize) -> io::Result<Vec<u8>> {
    use std::io::Read as _;
    if !std::fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(invalid());
    }
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > u64::try_from(maximum).unwrap() {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take(u64::try_from(maximum + 1).unwrap())
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(invalid());
    }
    Ok(bytes)
}

fn write(path: &Path, value: &Value) -> io::Result<()> {
    use std::io::Write as _;
    let temporary = path.with_extension("closed-part");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(value).map_err(|_| invalid())?)?;
    drop(file);
    let result = std::fs::hard_link(&temporary, path);
    std::fs::remove_file(temporary)?;
    result
}

fn keys(value: &Value, expected: &[&str]) -> io::Result<()> {
    let object = value.as_object().ok_or_else(invalid)?;
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        return Err(invalid());
    }
    Ok(())
}

async fn wait(directory: &Path, name: &str, deadline: Instant) -> io::Result<Value> {
    tokio::time::timeout_at(deadline, async {
        loop {
            let path = directory.join(name);
            if tokio::fs::try_exists(&path).await? {
                return serde_json::from_slice(&read(&path, 65536)?).map_err(|_| invalid());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("owned legacy actor handshake timed out"))?
}

fn validate(handoff: &Handoff, now: DateTime<Utc>) -> io::Result<()> {
    let hex = |text: &str, length| {
        text.len() == length
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    if handoff.version != 1
        || !hex(&handoff.nonce, 32)
        || handoff.database != "designs.sqlite"
        || handoff.source != "synthetic-client-source.ts"
        || !hex(&handoff.source_sha256, 64)
        || !hex(&handoff.package_sha256, 64)
        || handoff.source_bytes == 0
        || handoff.source_bytes > MAX_SOURCE
        || !handoff.source_bytes.is_multiple_of(188)
        || !handoff.owner_root.is_absolute()
    {
        return Err(invalid());
    }
    let ready = &handoff.ready_template;
    keys(
        ready,
        &[
            "probe_version",
            "scope",
            "source",
            "channels",
            "active_transcode_limit",
            "completion_file",
            "completion_timeout_seconds",
            "expected_completion",
            "client_actions",
            "limitations",
        ],
    )?;
    if ready["probe_version"] != 3
        || ready["active_transcode_limit"] != 4
        || ready["completion_timeout_seconds"] != 180
        || ready["completion_file"] != "client-complete.json"
    {
        return Err(invalid());
    }
    if ready["expected_completion"]
        != json!({"probe_version":3,"client_name":"VLC","client_version":"3.0.20",
        "decoder_version":"0.2.42","proof_suffix":"cumulative-effective-legacy-ordinary","termination":"owned-sigterm"})
    {
        return Err(invalid());
    }
    keys(
        &ready["source"],
        &["file", "sha256", "bytes", "duration_seconds"],
    )?;
    if ready["source"]
        != json!({"file":"source.ts","sha256":handoff.source_sha256,
        "bytes":handoff.source_bytes,"duration_seconds":60})
    {
        return Err(invalid());
    }
    let channels = ready["channels"]
        .as_array()
        .filter(|value| value.len() == 2)
        .ok_or_else(invalid)?;
    for (index, channel) in channels.iter().enumerate() {
        validate_channel(channel, index, now)?;
    }
    for field in [
        "profile_id",
        "design_id",
        "request_id",
        "generation_id",
        "root_receipt_id",
    ] {
        if channels[0][field] == channels[1][field] {
            return Err(invalid());
        }
    }
    if channels[0]["addon_id"] != ADDON || channels[1]["addon_id"] != ADDON {
        return Err(invalid());
    }
    Ok(())
}

fn validate_channel(channel: &Value, index: usize, now: DateTime<Utc>) -> io::Result<()> {
    keys(
        channel,
        &[
            "channel_id",
            "theme",
            "addon_id",
            "profile_id",
            "profile_revision",
            "theme_rgb4",
            "generation_id",
            "request_id",
            "design_id",
            "design_revision",
            "scoped_role",
            "original_design_id",
            "original_revision",
            "original_digest",
            "root_receipt_id",
            "root_receipt_digest",
            "original_valid_until",
            "expected_pages",
            "subcode",
            "expected_navigation",
            "edited_q_page",
            "edited_r_page",
            "edited_q_cell",
            "edited_r_cell",
            "url",
        ],
    )?;
    for key in [
        "profile_id",
        "profile_revision",
        "generation_id",
        "request_id",
        "design_id",
        "root_receipt_id",
    ] {
        if channel[key].as_i64().is_none_or(|id| id <= 0) {
            return Err(invalid());
        }
    }
    if channel["channel_id"] != index + 1
        || channel["theme"] != if index == 0 { "day" } else { "night" }
        || !channel["url"].is_null()
        || channel["scoped_role"] != 2
        || channel["original_revision"] != 1
        || channel["original_design_id"] != channel["design_id"]
        || channel["design_revision"] != if index == 0 { 3 } else { 1 }
        || channel["expected_pages"] != json!([200, 201])
        || channel["subcode"] != 0
        || channel["expected_navigation"] != json!([200, 201, 200])
    {
        return Err(invalid());
    }
    for key in ["original_digest", "root_receipt_digest"] {
        let digest = channel[key].as_str().ok_or_else(invalid)?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(invalid());
        }
    }
    if channel["theme_rgb4"]
        != if index == 0 {
            json!([15, 0, 0])
        } else {
            json!([0, 0, 15])
        }
        || channel["edited_q_page"] != if index == 0 { json!(200) } else { Value::Null }
        || channel["edited_r_page"] != if index == 0 { json!(200) } else { Value::Null }
        || channel["edited_q_cell"] != json!({"row":2,"column":8})
        || channel["edited_r_cell"] != json!({"row":3,"column":8})
    {
        return Err(invalid());
    }
    let expiry = DateTime::parse_from_rfc3339(
        channel["original_valid_until"]
            .as_str()
            .ok_or_else(invalid)?,
    )
    .map_err(|_| invalid())?;
    if expiry.offset().local_minus_utc() != 0 || expiry.with_timezone(&Utc) <= now {
        return Err(invalid());
    }
    Ok(())
}

async fn open(directory: &Path, handoff: &Handoff) -> io::Result<(AppState, Vec<u8>)> {
    validate(handoff, Utc::now())?;
    let root = handoff.owner_root.canonicalize()?;
    if root != handoff.owner_root
        || root.parent() != Some(std::env::temp_dir().canonicalize()?.as_path())
        || root
            .file_name()
            .and_then(|name| name.to_str())
            .is_none_or(|name| !name.starts_with(".tmp"))
    {
        return Err(invalid());
    }
    let marker: Value = serde_json::from_slice(&read(&root.join("legacy-owner.json"), 4096)?)
        .map_err(|_| invalid())?;
    if marker != json!({"version":1,"nonce":handoff.nonce,"proof_directory":directory}) {
        return Err(invalid());
    }
    for name in [&handoff.database, &handoff.source] {
        if root.join(name).canonicalize()? != root.join(name) {
            return Err(invalid());
        }
    }
    let source = read(&root.join(&handoff.source), MAX_SOURCE)?;
    if source.len() != handoff.source_bytes
        || hash(&source) != handoff.source_sha256
        || read(&directory.join("source.ts"), MAX_SOURCE)? != source
    {
        return Err(invalid());
    }
    if ersatzrs_infra::addons::calculate_addon_content_sha256(&root.join("package"))
        .await
        .map_err(|_| invalid())?
        != handoff.package_sha256
    {
        return Err(invalid());
    }
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(root.join(&handoff.database))
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await
        .map_err(|_| invalid())?;
    let schema = ersatzrs_infra::db::inspect_database_schema(&pool, DatabaseBackend::Sqlite)
        .await
        .map_err(|_| invalid())?;
    if !schema.is_ready() {
        pool.close().await;
        return Err(invalid());
    }
    let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
    let state = AppState::new(
        pool,
        DatabaseBackend::Sqlite,
        SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        },
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        uuid::Uuid::nil(),
        2,
        IptvAccessTokenVerifier::disabled(),
        sender,
        ersatzrs_core::locking::EntityLocker::new(),
    );
    qualify(&state, handoff, &root).await?;
    Ok((state, source))
}

async fn qualify(state: &AppState, handoff: &Handoff, root: &Path) -> io::Result<()> {
    let installed:(String,String,i64)=sqlx::query_as(r#"SELECT "InstallPath","ContentSha256","Enabled" FROM "AddonInstallation" WHERE "AddonId"=?"#)
        .bind(ADDON).fetch_one(&state.pool).await.map_err(|_|invalid())?;
    if Path::new(&installed.0) != root.join("package")
        || installed.1 != handoff.package_sha256
        || installed.2 != 1
    {
        return Err(invalid());
    }
    let source: String = sqlx::query_scalar(
        r#"SELECT "Path" FROM "MediaFile" WHERE "Id"=71 AND "MediaVersionId"=71"#,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| invalid())?;
    if Path::new(&source) != root.join(&handoff.source) {
        return Err(invalid());
    }
    let raw = state
        .application
        .get_optional_setting("teletext.output.policy")
        .await
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?;
    for channel in handoff.ready_template["channels"].as_array().unwrap() {
        qualify_channel(state, channel, &raw).await?;
    }
    if ersatzrs_infra::transcode_capacity::load_active_transcode_limit(&state.pool)
        .await
        .map_err(|_| invalid())?
        != Some(4)
    {
        return Err(invalid());
    }
    Ok(())
}

async fn qualify_channel(state: &AppState, channel: &Value, raw: &str) -> io::Result<()> {
    let policy = parse_native_teletext_runtime_policy(raw).map_err(|_| invalid())?;
    let accepted = reload_accepted_effective_teletext_generation(
        &state.application,
        channel["channel_id"].as_i64().unwrap(),
        policy.design,
        policy.encoding,
        std::time::Instant::now() + Duration::from_secs(10),
    )
    .await
    .map_err(|_| invalid())?
    .ok_or_else(invalid)?;
    let generation = accepted.generation();
    let request = &generation.request;
    if channel["generation_id"] != generation.id
        || channel["request_id"] != request.id
        || channel["design_id"] != request.design_id
        || channel["design_revision"] != request.revision
        || accepted
            .selection()
            .valid_until
            .map(|time| time.to_rfc3339())
            != channel["original_valid_until"].as_str().map(str::to_owned)
    {
        return Err(invalid());
    }
    let original = state
        .application
        .teletext_design_revision(request.design_id, 1, policy.design.max_serialized_bytes)
        .await
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?;
    if original.document_version != 2 || channel["original_digest"] != original.digest {
        return Err(invalid());
    }
    let ersatzrs_domain::TeletextEffectiveBindingSnapshot::Scripted(published) =
        &accepted.selection().bindings[0]
    else {
        return Err(invalid());
    };
    let ersatzrs_domain::TeletextOverrideState::Enabled(profile) = &published.interval.state else {
        return Err(invalid());
    };
    if channel["addon_id"] != profile.addon_id
        || channel["profile_id"] != profile.profile_id
        || channel["profile_revision"] != profile.profile_revision
    {
        return Err(invalid());
    }
    let receipt:(i64,i64,String,String)=sqlx::query_as(r#"SELECT "DesignId","Revision","DesignDigest","Digest" FROM "TeletextDesignExportReceipt" WHERE "Id"=?"#)
            .bind(channel["root_receipt_id"].as_i64().unwrap()).fetch_one(&state.pool).await.map_err(|_|invalid())?;
    if receipt.0 != request.design_id
        || receipt.1 != 1
        || receipt.2 != original.digest
        || channel["root_receipt_digest"] != receipt.3
    {
        return Err(invalid());
    }
    Ok(())
}

async fn cancel_responses(
    State(stop): State<CancellationToken>,
    request: Request,
    next: Next,
) -> Response {
    let streaming = matches!(
        request.uri().path(),
        "/iptv/channel/1.ts" | "/iptv/channel/2.ts"
    ) && request.uri().query() == Some("mode=ts-legacy");
    let response = next.run(request).await;
    if !streaming {
        return response;
    }
    let (parts, body) = response.into_parts();
    Response::from_parts(
        parts,
        Body::from_stream(body.into_data_stream().take_until(stop.cancelled_owned())),
    )
}

struct Origin {
    address: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<io::Result<()>>,
}
impl Origin {
    fn start(listener: TcpListener, app: Router, stop: CancellationToken) -> io::Result<Self> {
        let address = listener.local_addr()?;
        let shutdown = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
        });
        Ok(Self {
            address,
            stop,
            task,
        })
    }
    async fn close(&mut self) -> io::Result<()> {
        self.stop.cancel();
        match tokio::time::timeout(Duration::from_secs(5), &mut self.task).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(invalid()),
            Err(_) => {
                self.task.abort();
                let _ = (&mut self.task).await;
                Err(invalid())
            }
        }
    }
}
impl Drop for Origin {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}

async fn origins(state: &AppState) -> io::Result<(Origin, Origin)> {
    let internal = TcpListener::bind("127.0.0.1:0").await?;
    let public = TcpListener::bind("127.0.0.1:0").await?;
    state
        .internal_http_addr
        .set(internal.local_addr()?)
        .map_err(|_| invalid())?;
    state
        .streaming_http_addr
        .set(public.local_addr()?)
        .map_err(|_| invalid())?;
    state
        .next_engine
        .media
        .configure(internal.local_addr()?)
        .map_err(|_| invalid())?;
    let internal = Origin::start(
        internal,
        build_internal_app(state.clone()),
        CancellationToken::new(),
    )?;
    let stop = CancellationToken::new();
    let app = build_app(state.clone()).layer(middleware::from_fn_with_state(
        stop.clone(),
        cancel_responses,
    ));
    Ok((internal, Origin::start(public, app, stop)?))
}

async fn streams_closed(state: &AppState) -> io::Result<()> {
    tokio::time::timeout(Duration::from_secs(15), async {
        while active_transcode_snapshot(4).active != 0
            || !state.hls_sessions.active_channels().await.is_empty()
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| invalid())
}

async fn prefix(origin: &Origin, channel: i64) -> io::Result<Vec<u8>> {
    tokio::time::timeout(Duration::from_secs(25), async {
        let response = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| invalid())?
            .get(format!(
                "http://{}/iptv/channel/{channel}.ts?mode=ts-legacy",
                origin.address
            ))
            .send()
            .await
            .map_err(|_| invalid())?;
        if response.status() != axum::http::StatusCode::OK {
            return Err(invalid());
        }
        let mut body = response.bytes_stream();
        let mut bytes = Vec::new();
        while bytes.len() < 2 * 1024 * 1024 || !bytes.len().is_multiple_of(188) {
            let chunk = body
                .next()
                .await
                .ok_or_else(invalid)?
                .map_err(|_| invalid())?;
            if bytes.len() + chunk.len() > MAX_PREFIX {
                return Err(invalid());
            }
            bytes.extend_from_slice(&chunk);
        }
        drop(body);
        Ok(bytes)
    })
    .await
    .map_err(|_| invalid())?
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Complete {
    probe_version: u8,
    client_name: String,
    client_version: String,
    decoder_version: String,
    proof_suffix: String,
    termination: String,
}

fn completion_deadline(
    handoff: &Handoff,
    now: DateTime<Utc>,
    instant: Instant,
) -> io::Result<Instant> {
    let mut remaining = Duration::from_mins(3);
    for channel in handoff.ready_template["channels"]
        .as_array()
        .ok_or_else(invalid)?
    {
        let expiry = DateTime::parse_from_rfc3339(
            channel["original_valid_until"]
                .as_str()
                .ok_or_else(invalid)?,
        )
        .map_err(|_| invalid())?
        .with_timezone(&Utc);
        let scope_remaining = expiry
            .signed_duration_since(now)
            .to_std()
            .map_err(|_| invalid())?;
        if scope_remaining.is_zero() {
            return Err(invalid());
        }
        remaining = remaining.min(scope_remaining);
    }
    instant.checked_add(remaining).ok_or_else(invalid)
}

async fn observe(
    directory: &Path,
    handoff: &Handoff,
    state: &AppState,
    public: &Origin,
) -> io::Result<(String, [Vec<u8>; 2])> {
    let barrier = wait(
        directory,
        "owner-ready.json",
        Instant::now() + Duration::from_secs(30),
    )
    .await?;
    keys(&barrier, &["version", "nonce", "ready_sha256"])?;
    let instant = Instant::now();
    let deadline = completion_deadline(handoff, Utc::now(), instant)?;
    let ready = read(&directory.join("ready.json"), 65536)?;
    let ready_hash = hash(&ready);
    if barrier != json!({"version":1,"nonce":handoff.nonce,"ready_sha256":ready_hash}) {
        return Err(invalid());
    }
    let mut expected = handoff.ready_template.clone();
    for channel in expected["channels"].as_array_mut().unwrap() {
        channel["url"] = json!(format!(
            "http://{}/iptv/channel/{}.ts?mode=ts-legacy",
            public.address,
            channel["channel_id"].as_i64().unwrap()
        ));
    }
    if serde_json::from_slice::<Value>(&ready).map_err(|_| invalid())? != expected {
        return Err(invalid());
    }
    let complete = wait(directory, "client-complete.json", deadline).await?;
    let _: Complete = serde_json::from_value(complete.clone()).map_err(|_| invalid())?;
    if complete != expected["expected_completion"] {
        return Err(invalid());
    }
    streams_closed(state).await?;
    let day = prefix(public, 1).await?;
    streams_closed(state).await?;
    let night = prefix(public, 2).await?;
    streams_closed(state).await?;
    for (name, bytes) in [
        ("day.cold-prefix.ts", &day),
        ("night.cold-prefix.ts", &night),
    ] {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))?;
        file.write_all(bytes)?;
    }
    Ok((ready_hash, [day, night]))
}

async fn verify(source: &[u8], captures: &[Vec<u8>; 2]) {
    let ffmpeg = exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let tsanalyze = exact_tool("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    let hashes: BTreeSet<_> = checks::video_hashes(&ffmpeg, source)
        .await
        .into_iter()
        .collect();
    for (index, bytes) in captures.iter().enumerate() {
        checks::verify_av(&ffmpeg, &hashes, bytes).await;
        checks::verify_transport(&tsanalyze, bytes, true).await;
        for page in ["200", "201"] {
            let text = String::from_utf8(decode_text(&ffmpeg, bytes, page, "0:s:0").await).unwrap();
            assert!(text.contains("AZ"));
            assert_eq!(
                text.lines().any(|line| line.trim() == "Q"),
                index == 0 && page == "200"
            );
            assert_eq!(
                text.lines().any(|line| line.trim() == "R"),
                index == 0 && page == "200"
            );
        }
    }
}

async fn run(directory: PathBuf) {
    assert_eq!(
        ersatzrs_ffmpeg::ffmpeg_path(),
        exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG")
    );
    assert_eq!(ersatzrs_ffmpeg::ffprobe_path(), exact_tool("FFPROBE_PATH"));
    let handoff = wait(
        &directory,
        "owner-handoff.json",
        Instant::now() + Duration::from_mins(1),
    )
    .await
    .unwrap();
    let directory = directory.canonicalize().unwrap();
    let handoff: Handoff = serde_json::from_value(handoff).unwrap();
    let (state, source) = Box::pin(open(&directory, &handoff)).await.unwrap();
    let (mut internal, mut public) = origins(&state).await.unwrap();
    let result=AssertUnwindSafe(async {
        write(&directory.join("actor-ready.json"),&json!({"version":1,"nonce":handoff.nonce,
            "public_address":public.address.to_string(),"internal_address":internal.address.to_string()}))?;
        observe(&directory,&handoff,&state,&public).await
    }).catch_unwind().await;
    let public_closed = public.close().await;
    let actors_closed = streams_closed(&state).await;
    let internal_closed = internal.close().await;
    state.pool.close().await;
    let (ready_hash, captures) = match result {
        Ok(result) => result.unwrap(),
        Err(panic) => std::panic::resume_unwind(panic),
    };
    public_closed.unwrap();
    actors_closed.unwrap();
    internal_closed.unwrap();
    verify(&source, &captures).await;
    write(&directory.join("actor-done.json"),&json!({"version":1,"nonce":handoff.nonce,"ready_sha256":ready_hash,
        "source_sha256":hash(&source),"capture_sha256":[hash(&captures[0]),hash(&captures[1])],
        "capture_bytes":[captures[0].len(),captures[1].len()],"capacity_active":active_transcode_snapshot(4).active,
        "public_registry_empty":state.hls_sessions.active_channels().await.is_empty(),"listeners_joined":true})).unwrap();
}

#[test]
#[ignore = "Requires a simultaneous genuine prepared-owner integration test, pinned tools and root-inspected two-client owned-SIGTERM receipt"]
fn scoped_legacy_client_actor_serves_handoff_at_real_ordinary_urls() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    let directory = PathBuf::from(exact_tool(DIRECTORY_ENV));
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(run(directory));
    assert_eq!(active_transcode_snapshot(4).active, 0);
}

fn sample(now: DateTime<Utc>) -> Value {
    let channels=(0..2).map(|index|json!({"channel_id":index+1,"theme":if index==0 {"day"}else{"night"},
        "addon_id":ADDON,"profile_id":index+1,"profile_revision":2,"theme_rgb4":if index==0 {[15,0,0]}else{[0,0,15]},
        "generation_id":index+10,"request_id":index+20,"design_id":index+30,"design_revision":if index==0 {3}else{1},
        "scoped_role":2,"original_design_id":index+30,"original_revision":1,"original_digest":"a".repeat(64),
        "root_receipt_id":index+40,"root_receipt_digest":"b".repeat(64),"original_valid_until":(now+chrono::Duration::seconds(240)).to_rfc3339(),
        "expected_pages":[200,201],"subcode":0,"expected_navigation":[200,201,200],
        "edited_q_page":if index==0 {Some(200)}else{None},"edited_r_page":if index==0 {Some(200)}else{None},
        "edited_q_cell":{"row":2,"column":8},"edited_r_cell":{"row":3,"column":8},"url":null})).collect::<Vec<_>>();
    json!({"version":1,"nonce":"c".repeat(32),"owner_root":std::env::temp_dir().join(".tmp-owned-fixture"),
        "database":"designs.sqlite","source":"synthetic-client-source.ts","source_sha256":"d".repeat(64),"source_bytes":188,
        "package_sha256":"e".repeat(64),"ready_template":{"probe_version":3,"scope":"synthetic closed handoff validator only",
            "source":{"file":"source.ts","sha256":"d".repeat(64),"bytes":188,"duration_seconds":60},"channels":channels,
            "active_transcode_limit":4,"completion_file":"client-complete.json","completion_timeout_seconds":180,
            "expected_completion":{"probe_version":3,"client_name":"VLC","client_version":"3.0.20","decoder_version":"0.2.42",
                "proof_suffix":"cumulative-effective-legacy-ordinary","termination":"owned-sigterm"},"client_actions":"fixture",
            "limitations":"Pure protocol validation grants no database/publication/client authority."}})
}

fn checked(value: Value, now: DateTime<Utc>) -> bool {
    serde_json::from_value::<Handoff>(value)
        .ok()
        .is_some_and(|handoff| validate(&handoff, now).is_ok())
}

#[test]
fn scoped_legacy_handoff_accepts_closed_original_identity_without_urls() {
    let now = Utc::now();
    assert!(checked(sample(now), now));
}

#[test]
fn scoped_legacy_handoff_rejects_unknown_fields_and_versions() {
    let now = Utc::now();
    let mut value = sample(now);
    value["unexpected"] = json!(true);
    assert!(!checked(value, now));
    let mut value = sample(now);
    value["version"] = json!(2);
    assert!(!checked(value, now));
    let mut value = sample(now);
    value["ready_template"]["channels"][0]["unexpected"] = json!(true);
    assert!(!checked(value, now));
}

#[test]
fn scoped_legacy_handoff_rejects_relative_owners_and_path_escape() {
    let now = Utc::now();
    let mut value = sample(now);
    value["owner_root"] = json!("relative");
    assert!(!checked(value, now));
    let mut value = sample(now);
    value["database"] = json!("../designs.sqlite");
    assert!(!checked(value, now));
    let mut value = sample(now);
    value["source"] = json!("../source.ts");
    assert!(!checked(value, now));
}

#[test]
fn scoped_legacy_handoff_rejects_unbounded_or_unaligned_source() {
    let now = Utc::now();
    for bytes in [0, 187, MAX_SOURCE + 188] {
        let mut value = sample(now);
        value["source_bytes"] = json!(bytes);
        assert!(!checked(value, now));
    }
    let mut value = sample(now);
    value["source_sha256"] = json!("f".repeat(63));
    assert!(!checked(value, now));
}

#[test]
fn scoped_legacy_handoff_rejects_relabelled_roots_and_bool_ids() {
    let now = Utc::now();
    let mut value = sample(now);
    value["ready_template"]["channels"][0]["original_design_id"] = json!(999);
    assert!(!checked(value, now));
    let mut value = sample(now);
    let first = value["ready_template"]["channels"][0]["root_receipt_id"].clone();
    value["ready_template"]["channels"][1]["root_receipt_id"] = first;
    assert!(!checked(value, now));
    let mut value = sample(now);
    value["ready_template"]["channels"][0]["profile_id"] = json!(true);
    assert!(!checked(value, now));
}

#[test]
fn scoped_legacy_handoff_rejects_stale_scope_and_unowned_client_identity() {
    let now = Utc::now();
    let mut value = sample(now);
    value["ready_template"]["channels"][0]["original_valid_until"] = now.to_rfc3339().into();
    assert!(!checked(value, now));
    for termination in ["eof", "owned-qt-quit"] {
        let mut value = sample(now);
        value["ready_template"]["expected_completion"]["termination"] = json!(termination);
        assert!(!checked(value, now));
    }
}

#[test]
fn scoped_legacy_handoff_accepts_short_future_scope_and_clamps_to_earliest_original_expiry() {
    let now = Utc::now();
    let instant = Instant::now();
    let mut value = sample(now);
    value["ready_template"]["channels"][0]["original_valid_until"] =
        (now + chrono::Duration::seconds(15)).to_rfc3339().into();
    value["ready_template"]["channels"][1]["original_valid_until"] =
        (now + chrono::Duration::seconds(10)).to_rfc3339().into();
    assert!(checked(value.clone(), now));
    let handoff: Handoff = serde_json::from_value(value).unwrap();
    assert_eq!(
        completion_deadline(&handoff, now, instant).unwrap(),
        instant + Duration::from_secs(10)
    );
    assert!(completion_deadline(&handoff, now + chrono::Duration::seconds(10), instant).is_err());
}
