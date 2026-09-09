//! Native-tool admission for managed add-on invocations (ADR 0085, AMM-N-01).

use super::*;
use crate::addon_tool_gate::{ToolLease, ToolLeaseMode};
use ersatzrs_addon_contract::{ToolUpdateRequest, ToolUpdateResult, MAX_TOOL_UPDATE_OUTPUT_BYTES};

pub(crate) fn acquire_tool_lease(
    paths: &[PathBuf],
    mode: ToolLeaseMode,
) -> Result<Option<ToolLease>, AddonError> {
    if paths.is_empty() {
        return Ok(None);
    }
    let root = crate::file_system_layout::app_state_folder()
        .join("addons")
        .join("tool-leases");
    ToolLease::try_acquire(&root, paths, mode)
        .map_err(|_| AddonError::ToolCleanupRequired)?
        .map(Some)
        .ok_or(AddonError::ToolBusy)
}

pub(super) fn spawn_bounded_addon_process(
    command: &ResolvedAddonCommand,
    has_input: bool,
    lease_mode: ToolLeaseMode,
) -> Result<ManagedAddonChild, AddonError> {
    let mut process = addon_process(command);
    process
        .current_dir(&command.working_directory)
        .env_clear()
        .envs(addon_process_environment())
        .envs(&command.environment)
        .stdin(if has_input {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // No await between durable receipt creation, spawn, and child ownership.
    // Cancellation before this poll therefore cannot leave a false receipt.
    let lease = acquire_tool_lease(&command.tool_paths, lease_mode)?;
    if let Ok(mut child) = spawn_managed_addon_process(process) {
        if let Some(lease) = lease {
            child.retain_tool_lease(lease);
        }
        Ok(child)
    } else {
        if let Some(lease) = lease {
            let _ = lease.confirm_cleanup();
        }
        Err(AddonError::Process(
            "entrypoint could not be started".to_owned(),
        ))
    }
}

pub(super) async fn resolve_tool_paths(
    manifest: &AddonManifest,
    environment: &mut BTreeMap<String, String>,
    working_directory: &Path,
) -> Result<Vec<PathBuf>, AddonError> {
    let mut paths = Vec::new();
    for setting in manifest
        .settings
        .iter()
        .filter(|setting| setting.kind == AddonSettingKind::Executable)
    {
        let key = format!("ERSATZRS_ADDON_SETTING_{}", setting.key);
        if let Some(value) = environment.get(&key).cloned() {
            if value.trim().is_empty() {
                continue;
            }
            let path = executable_path(&value, working_directory)
                .await?
                .ok_or_else(|| {
                    AddonError::operation(
                        OPERATION_CODE_MISSING_SETTING,
                        format!(
                            "configured executable {} is unavailable; check the executable setting",
                            setting.key
                        ),
                    )
                })?;
            // The provider must invoke the executable whose identity is
            // leased, not resolve the original basename through another PATH.
            let value = path
                .to_str()
                .ok_or(AddonError::ToolCleanupRequired)?
                .to_owned();
            environment.insert(key, value);
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

async fn executable_path(
    value: &str,
    working_directory: &Path,
) -> Result<Option<PathBuf>, AddonError> {
    let path = Path::new(value);
    let candidates = if path.is_absolute() {
        vec![path.to_path_buf()]
    } else if path.components().count() > 1 {
        vec![working_directory.join(path)]
    } else {
        let environment = addon_process_environment();
        let mut candidates = Vec::new();
        if let Some(search) = environment.get("PATH") {
            for directory in std::env::split_paths(search) {
                candidates.push(directory.join(path));
                #[cfg(windows)]
                if path.extension().is_none() {
                    for extension in ["exe", "com", "bat", "cmd"] {
                        candidates.push(directory.join(path).with_extension(extension));
                    }
                }
            }
        }
        candidates
    };
    for candidate in candidates {
        match tokio::fs::metadata(&candidate).await {
            Ok(metadata) if metadata.is_file() => {
                return canonicalize_addon_path(&candidate)
                    .await
                    .map(Some)
                    .map_err(|_| AddonError::ToolCleanupRequired);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(AddonError::ToolCleanupRequired),
        }
    }
    // The caller turns absence into a structured readiness diagnostic. Running
    // an unleased provider here could race a temporarily missing replacement.
    Ok(None)
}

/// Run an explicitly permitted native update only after idle exclusive admission.
///
/// # Errors
/// Returns busy without draining readers, cleanup-required on uncertain prior
/// process exit, or a bounded invocation/contract failure. No replacement bytes
/// are downloaded or installed by the host.
pub async fn invoke_tool_update(
    pool: &DbPool,
    addon_id: &str,
    tool_key: &str,
    runtime: &AddonRuntimeContext,
    mut cancellation: tokio::sync::oneshot::Receiver<()>,
) -> Result<ToolUpdateResult, AddonError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    // Preflight owns no process or receipt, so it is safe to drop on cancellation.
    // Include package hashing and database waits in the same invocation budget.
    let (command, input) = tokio::select! {
        biased;
        _ = &mut cancellation => return Err(AddonError::Process("entrypoint cancelled".to_owned())),
        result = tokio::time::timeout_at(deadline, prepare_tool_update(pool, addon_id, tool_key, runtime)) => {
            result.map_err(|_| AddonError::Process("entrypoint timed out".to_owned()))??
        }
    };
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
        return Err(AddonError::Process("entrypoint timed out".to_owned()));
    }
    // Once a process exists the runner owns cancellation and confirmed cleanup.
    let output = run_bounded_command_inner(
        &command,
        remaining,
        MAX_TOOL_UPDATE_OUTPUT_BYTES,
        Some(&input),
        &[],
        Some(cancellation),
        ToolLeaseMode::Update,
    )
    .await?;
    let result = ToolUpdateResult::from_json(&output)
        .map_err(|_| AddonError::Process("invalid tool update result".to_owned()))?;
    if result.tool_key != tool_key {
        return Err(AddonError::Process(
            "tool update result identity mismatch".to_owned(),
        ));
    }
    Ok(result)
}

async fn prepare_tool_update(
    pool: &DbPool,
    addon_id: &str,
    tool_key: &str,
    runtime: &AddonRuntimeContext,
) -> Result<(ResolvedAddonCommand, Vec<u8>), AddonError> {
    let mut command =
        resolve_enabled_addon_command(pool, addon_id, CAPABILITY_TOOL_UPDATE_V1, "update", runtime)
            .await?;
    let installation = load_installed_addon(pool, addon_id)
        .await?
        .ok_or_else(|| AddonError::NotFound("add-on is not installed".to_owned()))?;
    if !installation.enabled
        || installation.state_revision != command.state_revision
        || !installation
            .manifest
            .permissions
            .tool_update
            .iter()
            .any(|key| key == tool_key)
    {
        return Err(AddonError::Validation(
            "tool update permission is unavailable".to_owned(),
        ));
    }
    let value = command
        .environment
        .get(&format!("ERSATZRS_ADDON_SETTING_{tool_key}"))
        .ok_or_else(|| AddonError::Validation("tool setting is unavailable".to_owned()))?;
    let tool = executable_path(value, &command.working_directory)
        .await?
        .ok_or_else(|| AddonError::Process("configured update tool is unavailable".to_owned()))?;
    // Restrict this maintenance invocation to the requested declared executable.
    command.tool_paths = vec![tool];
    command.environment.insert(
        "ERSATZRS_ADDON_CAPABILITY".to_owned(),
        CAPABILITY_TOOL_UPDATE_V1.to_owned(),
    );
    let request = ToolUpdateRequest {
        schema: CAPABILITY_TOOL_UPDATE_V1.to_owned(),
        tool_key: tool_key.to_owned(),
    };
    request
        .validate()
        .map_err(|_| AddonError::Validation("invalid tool update request".to_owned()))?;
    let input = serde_json::to_vec(&request)
        .map_err(|_| AddonError::Validation("invalid tool update request".to_owned()))?;
    let current = load_installed_addon(pool, addon_id).await;
    if !matches!(current, Ok(Some(ref current)) if current.enabled && current.state_revision == command.state_revision)
    {
        return Err(AddonError::Validation(
            "tool update configuration changed".to_owned(),
        ));
    }
    Ok((command, input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn update_cancellation_interrupts_database_preflight() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let _occupied = pool.acquire().await.unwrap();
        let (cancel, cancellation) = tokio::sync::oneshot::channel();
        let runtime = AddonRuntimeContext::default();
        let invocation = invoke_tool_update(
            &pool,
            "org.example.fixture",
            "TOOL_BIN",
            &runtime,
            cancellation,
        );
        tokio::pin!(invocation);
        assert!(futures_util::poll!(&mut invocation).is_pending());
        cancel.send(()).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), invocation)
            .await
            .unwrap();
        assert!(
            matches!(result, Err(AddonError::Process(message)) if message == "entrypoint cancelled")
        );
    }

    #[tokio::test]
    async fn cancelled_update_does_not_acquire_a_pre_spawn_receipt() {
        let temporary = tempfile::tempdir().unwrap();
        let command = ResolvedAddonCommand {
            addon_id: "org.example.tool-gate".to_owned(),
            program: "must-not-spawn".to_owned(),
            args: Vec::new(),
            working_directory: temporary.path().to_path_buf(),
            environment: BTreeMap::new(),
            state_revision: "fixture".to_owned(),
            // Acquiring this nonexistent executable would fail before spawn.
            tool_paths: vec![temporary.path().join("missing-tool")],
        };
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        cancel.send(()).unwrap();
        let result = run_bounded_command_inner(
            &command,
            Duration::from_secs(1),
            1024,
            None,
            &[],
            Some(cancelled),
            ToolLeaseMode::Update,
        )
        .await;
        assert!(
            matches!(result, Err(AddonError::Process(message)) if message == "entrypoint cancelled")
        );
    }

    // These fixtures use the existing Unix process-group counterpart to the
    // Windows tracking-job implementation, without launching fixture tool files.
    #[cfg(unix)]
    #[tokio::test]
    async fn managed_invocation_holds_tool_lease_until_descendant_cleanup() {
        let temporary = tempfile::tempdir().unwrap();
        let tool = temporary.path().join("tool-identity");
        std::fs::write(&tool, b"identity only").unwrap();
        let tools = vec![tool];
        let root = temporary.path().join("gate");
        let lease = ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Use)
            .unwrap()
            .unwrap();
        let mut process = Command::new("/bin/sh");
        process.args(["-c", "sleep 60 & printf ready; wait"]);
        process.stdout(Stdio::piped());
        let mut child = spawn_managed_addon_process(process).unwrap();
        child.retain_tool_lease(lease);
        let mut stdout = child.stdout().take().unwrap();
        let mut ready = [0; 5];
        tokio::time::timeout(
            Duration::from_secs(5),
            tokio::io::AsyncReadExt::read_exact(&mut stdout, &mut ready),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&ready, b"ready");
        assert!(ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Update)
            .unwrap()
            .is_none());
        child.kill().await.unwrap();
        ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Update)
            .unwrap()
            .unwrap()
            .confirm_cleanup()
            .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dropped_stream_child_keeps_tool_receipt_until_cleanup_is_confirmed() {
        let temporary = tempfile::tempdir().unwrap();
        let tool = temporary.path().join("tool-identity");
        std::fs::write(&tool, b"identity only").unwrap();
        let tools = vec![tool];
        let root = temporary.path().join("gate");
        let lease = ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Use)
            .unwrap()
            .unwrap();
        let mut process = Command::new("/bin/sh");
        process.args(["-c", "exec sleep 60"]);
        let mut child = spawn_managed_addon_process(process).unwrap();
        child.retain_tool_lease(lease);
        drop(child);
        tokio::time::timeout(Duration::from_secs(7), async {
            loop {
                if let Some(lease) =
                    ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Update).unwrap()
                {
                    lease.confirm_cleanup().unwrap();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("drop cleanup releases the tool only after process exit");
    }
}
