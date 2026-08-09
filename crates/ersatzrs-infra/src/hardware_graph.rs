//! Run and persist operation-scoped hardware-graph probes (ADR 0014, milestone
//! `p8b-01`).
//!
//! The pure model — which operations exist, their probe commands, and the
//! failure classifier — lives in `ersatzrs_ffmpeg::graph_probe`. This module
//! owns spawning the probe, bounding it, and persisting the learned denials,
//! mirroring the existing `HardwareDecodeDenial` split.

use std::path::Path;
use std::time::Duration;

use ersatzrs_ffmpeg::{
    FfmpegCapabilityReport, FfmpegRuntimeReport, GraphFailureKind, HardwareGraphDenial,
    HardwareGraphOperation,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tokio::sync::RwLock;

use crate::db::DbPool;

const CONFIG_KEY_HARDWARE_GRAPH_DENIALS: &str = "ffmpeg.hardware.graph_denials";

/// Upper bound for one functional probe.
///
/// The probe encodes a single 256x144 frame from `lavfi`, so a healthy run is
/// well under a second. A hung Vulkan or CUDA driver is exactly the condition
/// being probed for, so the timeout must be treated as a denial rather than as
/// a reason to keep waiting at startup.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, thiserror::Error)]
pub enum HardwareGraphDenialError {
    #[error("database error while loading hardware graph denials: {0}")]
    Database(#[from] sqlx::Error),
    #[error("failed to encode hardware graph denials: {0}")]
    Encode(#[source] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedHardwareGraphDenial {
    operation: String,
    #[serde(default)]
    device: String,
    kind: String,
}

impl From<&HardwareGraphDenial> for PersistedHardwareGraphDenial {
    fn from(value: &HardwareGraphDenial) -> Self {
        Self {
            operation: value.operation.id().to_owned(),
            device: value.device.clone(),
            kind: failure_kind_name(value.kind).to_owned(),
        }
    }
}

impl PersistedHardwareGraphDenial {
    fn into_denial(self) -> Option<HardwareGraphDenial> {
        Some(HardwareGraphDenial::new(
            HardwareGraphOperation::from_id(&self.operation)?,
            self.device,
            failure_kind_from_name(&self.kind)?,
        ))
    }
}

/// Persisted graph-probe record.
///
/// Carries completion state and positively proven operations, not just
/// denials. An empty denial list is ambiguous on its own — it means both "every
/// graph worked" and "nothing has been probed" — so reusing it as a cache would
/// treat an unprobed machine as fully proven.
///
/// Keyed by the identities ADR 0014 requires: the `ffmpeg` binary and the
/// device. A record whose identities do not match the current runtime is not
/// reusable, and every applicable operation must have terminal evidence
/// (proven or denied) before the record counts as complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct PersistedHardwareGraphState {
    #[serde(default)]
    ffmpeg_path: Option<String>,
    /// Device the probes ran against, so a GPU swap re-probes.
    #[serde(default)]
    device: Option<String>,
    /// Operations that ran to completion. `None` on a legacy row written before
    /// completion was tracked, which is treated as incomplete.
    #[serde(default)]
    proven: Option<Vec<String>>,
    /// Operations whose probe was attempted, including unclassified failures.
    /// This preserves the distinction between "never probed" and "probed but
    /// unproven" without making a partial cache reusable on the next startup.
    #[serde(default)]
    attempted: Option<Vec<String>>,
    #[serde(default)]
    denials: Vec<PersistedHardwareGraphDenial>,
}

impl PersistedHardwareGraphState {
    /// Whether this record can be reused instead of re-probing.
    ///
    /// Requires matching identities and terminal evidence for every operation
    /// that applies. A partial record — one where a probe returned `Unproven`
    /// and so recorded neither proof nor denial — is deliberately not reusable.
    fn is_reusable_for(
        &self,
        ffmpeg_path: &str,
        device: &str,
        applicable: &[HardwareGraphOperation],
    ) -> bool {
        if self.ffmpeg_path.as_deref() != Some(ffmpeg_path) {
            return false;
        }
        // A record written before device identity was tracked cannot be
        // attributed, so it is not reusable.
        let Some(recorded_device) = self.device.as_deref() else {
            return false;
        };
        if !recorded_device.eq_ignore_ascii_case(device) {
            return false;
        }
        let Some(proven) = self.proven.as_ref() else {
            return false;
        };

        applicable.iter().all(|operation| {
            proven.iter().any(|id| id == operation.id())
                || self
                    .denials
                    .iter()
                    .any(|denial| denial.operation == operation.id())
        })
    }
}

const fn failure_kind_name(kind: GraphFailureKind) -> &'static str {
    match kind {
        GraphFailureKind::VulkanDevice => "vulkan-device",
        GraphFailureKind::Libplacebo => "libplacebo",
        GraphFailureKind::HardwareMap => "hardware-map",
        GraphFailureKind::CudaInterop => "cuda-interop",
        GraphFailureKind::EncoderInit => "encoder-init",
        GraphFailureKind::MissingFilter => "missing-filter",
    }
}

fn failure_kind_from_name(value: &str) -> Option<GraphFailureKind> {
    match value {
        "vulkan-device" => Some(GraphFailureKind::VulkanDevice),
        "libplacebo" => Some(GraphFailureKind::Libplacebo),
        "hardware-map" => Some(GraphFailureKind::HardwareMap),
        "cuda-interop" => Some(GraphFailureKind::CudaInterop),
        "encoder-init" => Some(GraphFailureKind::EncoderInit),
        "missing-filter" => Some(GraphFailureKind::MissingFilter),
        _ => None,
    }
}

/// Outcome of one functional graph probe.
///
/// The three states are deliberately distinct. Collapsing "worked" and "failed
/// in a way I could not classify" into one value is what let an unclassified
/// failure read as positive evidence: absence of a denial meant selectable, so
/// a graph that had actually failed stayed eligible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareGraphProbe {
    /// The graph ran to completion. This is the only positive evidence.
    Proven,
    /// The graph failed with a recognised signature. Safe to persist, because
    /// the same binary on the same device will fail the same way.
    Denied(HardwareGraphDenial),
    /// The graph failed in a way the classifier does not recognise. Not
    /// selectable for this process, but *not* persisted: ADR 0014 forbids
    /// learning a denial from an arbitrary `FFmpeg` failure, and a transient
    /// error must not permanently disable a working graph. The next capability
    /// refresh re-probes.
    Unproven,
}

impl HardwareGraphProbe {
    /// True only for a graph that demonstrably ran.
    #[must_use]
    pub const fn is_proven(&self) -> bool {
        matches!(self, Self::Proven)
    }

    /// The denial to persist, if this outcome produced one.
    #[must_use]
    pub const fn denial(&self) -> Option<&HardwareGraphDenial> {
        match self {
            Self::Denied(denial) => Some(denial),
            Self::Proven | Self::Unproven => None,
        }
    }
}

/// Run one bounded functional graph probe.
pub async fn probe_hardware_graph(
    ffmpeg_path: &Path,
    operation: HardwareGraphOperation,
    device: &str,
) -> HardwareGraphProbe {
    let mut command = tokio::process::Command::new(ffmpeg_path);
    command.args(operation.probe_arguments());
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::null());
    command.stderr(std::process::Stdio::piped());

    let output = match tokio::time::timeout(PROBE_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            // A binary that will not spawn is an environment problem, not a
            // property of the graph, so it is not persisted as a denial.
            tracing::warn!(
                operation = operation.id(),
                %error,
                "hardware graph probe could not spawn ffmpeg; treating the graph as unproven"
            );
            return HardwareGraphProbe::Unproven;
        }
        Err(_) => {
            // A hung driver is exactly what the probe exists to catch, and it
            // recurs, so a timeout is a real classified denial.
            tracing::warn!(
                operation = operation.id(),
                timeout_secs = PROBE_TIMEOUT.as_secs(),
                "hardware graph probe timed out; denying the graph"
            );
            return HardwareGraphProbe::Denied(HardwareGraphDenial::new(
                operation,
                device,
                GraphFailureKind::VulkanDevice,
            ));
        }
    };

    if output.status.success() {
        return HardwareGraphProbe::Proven;
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let Some(kind) = GraphFailureKind::classify(&stderr) else {
        tracing::warn!(
            operation = operation.id(),
            device,
            "hardware graph probe failed with an unrecognised signature; treating the graph as unproven"
        );
        return HardwareGraphProbe::Unproven;
    };
    tracing::info!(
        operation = operation.id(),
        device,
        reason = kind.reason(),
        "hardware graph probe denied a graph"
    );
    HardwareGraphProbe::Denied(HardwareGraphDenial::new(operation, device, kind))
}

/// Probe every operation that the advertised capabilities make plausible and
/// fold the denials into the report.
///
/// Only graphs whose prerequisites are advertised get probed: there is no point
/// spawning a Vulkan probe on a build without the `vulkan` hwaccel, and a
/// platform without Vulkan must stay a normal, healthy capability result rather
/// than a denial or a startup failure.
pub async fn refresh_hardware_graph_capabilities(
    ffmpeg_path: &Path,
    report: &mut FfmpegCapabilityReport,
) {
    let Some(operations) = applicable_graph_operations(report) else {
        return;
    };
    let device = report.hardware_graph_device();

    for operation in operations {
        report.record_hardware_graph_probe_attempt(operation, &device);
        match probe_hardware_graph(ffmpeg_path, operation, &device).await {
            HardwareGraphProbe::Proven => report.prove_hardware_graph(operation, &device),
            HardwareGraphProbe::Denied(denial) => report.deny_hardware_graph(denial),
            // Left with neither proof nor denial: not selectable now, and not
            // persisted, so the next refresh re-probes.
            HardwareGraphProbe::Unproven => {}
        }
    }
}

/// Graphs worth probing on this runtime, or `None` when none apply.
///
/// Only graphs whose prerequisites are advertised are probed: a build without
/// the `vulkan` hwaccel or the libplacebo filter cannot run that graph, and a
/// platform without Vulkan must stay a normal capability result rather than a
/// denial.
fn applicable_graph_operations(
    report: &FfmpegCapabilityReport,
) -> Option<Vec<HardwareGraphOperation>> {
    if !report.has_hwaccel("cuda") {
        return None;
    }
    let mut operations = Vec::new();
    if report.has_hwaccel("vulkan") && report.filters.contains("libplacebo") {
        operations.push(HardwareGraphOperation::NvencHdrVulkanLibplacebo);
    }
    operations.push(HardwareGraphOperation::NvencCudaScale);
    Some(operations)
}

/// Load persisted graph denials, ignoring malformed rows.
///
/// # Errors
///
/// Returns an error when the database read fails.
pub async fn load_hardware_graph_denials(
    pool: &DbPool,
) -> Result<Vec<HardwareGraphDenial>, HardwareGraphDenialError> {
    let Some(raw) = load_config(pool).await? else {
        return Ok(Vec::new());
    };
    let state: PersistedHardwareGraphState = serde_json::from_str(&raw).unwrap_or_default();
    Ok(state
        .denials
        .into_iter()
        .filter_map(PersistedHardwareGraphDenial::into_denial)
        .collect())
}

/// Apply a persisted graph record to a startup report.
///
/// Returns `true` when a complete, identity-matching record was reused, and
/// `false` when the caller should re-probe. A stale, partial, or legacy record
/// is cleared rather than partially applied — an empty denial list is not proof
/// that every graph works.
///
/// # Errors
///
/// Returns an error when the database read or write fails.
pub async fn apply_hardware_graph_denials(
    pool: &DbPool,
    ffmpeg_path: &str,
    report: &mut FfmpegCapabilityReport,
) -> Result<bool, HardwareGraphDenialError> {
    let Some(raw) = load_config(pool).await? else {
        return Ok(false);
    };
    let state: PersistedHardwareGraphState = serde_json::from_str(&raw).unwrap_or_default();

    let device = report.hardware_graph_device();
    let applicable = applicable_graph_operations(report).unwrap_or_default();

    if !state.is_reusable_for(ffmpeg_path, &device, &applicable) {
        // Stale identity, a partial record, or a legacy row. Discard it so the
        // caller re-probes rather than trusting incomplete evidence.
        tracing::info!(
            "stored hardware graph record is not reusable for this runtime; clearing to re-probe"
        );
        clear_hardware_graph_denials(pool).await?;
        return Ok(false);
    }

    for denial in state
        .denials
        .into_iter()
        .filter_map(PersistedHardwareGraphDenial::into_denial)
    {
        report.deny_hardware_graph(denial);
    }
    for operation in applicable {
        if !report.hardware_graph_denied(operation, &device) {
            report.prove_hardware_graph(operation, &device);
        }
    }
    Ok(true)
}

/// Clear persisted graph denials after an explicit capability refresh.
///
/// # Errors
///
/// Returns an error when the database write fails.
pub async fn clear_hardware_graph_denials(pool: &DbPool) -> Result<(), HardwareGraphDenialError> {
    upsert_config(pool, "").await
}

/// Learn one graph denial in a worker snapshot and the process-wide runtime,
/// then persist the merged runtime state.
///
/// Stream supervisors build from local snapshots that can become stale while
/// another stream is learning a different denial. The shared write guard is
/// deliberately held through persistence so two supervisors cannot overwrite
/// each other's complete database snapshots. The local and shared reports stay
/// updated even if persistence fails, so the current process still avoids the
/// failed graph.
///
/// # Errors
///
/// Returns an error when encoding or the database write fails.
pub async fn persist_hardware_graph_denial_into_runtime(
    pool: &DbPool,
    ffmpeg_path: &str,
    report: &mut FfmpegCapabilityReport,
    runtime: &RwLock<FfmpegRuntimeReport>,
    denial: HardwareGraphDenial,
) -> Result<(), HardwareGraphDenialError> {
    report.deny_hardware_graph(denial.clone());
    let mut runtime = runtime.write().await;
    runtime.capabilities.deny_hardware_graph(denial);
    report.clone_from(&runtime.capabilities);
    persist_hardware_graph_denials(pool, ffmpeg_path, &runtime.capabilities).await
}

/// Persist the report's graph denials against the current `ffmpeg` path.
///
/// # Errors
///
/// Returns an error when encoding or the database write fails.
pub async fn persist_hardware_graph_denials(
    pool: &DbPool,
    ffmpeg_path: &str,
    report: &FfmpegCapabilityReport,
) -> Result<(), HardwareGraphDenialError> {
    let state = PersistedHardwareGraphState {
        ffmpeg_path: Some(ffmpeg_path.to_owned()),
        device: Some(report.hardware_graph_device()),
        proven: Some(
            report
                .proven_hardware_graphs
                .iter()
                .map(|(operation, _)| operation.id().to_owned())
                .collect(),
        ),
        attempted: Some(
            report
                .attempted_hardware_graphs
                .iter()
                .map(|(operation, _)| operation.id().to_owned())
                .collect(),
        ),
        denials: report
            .hardware_graph_denials
            .iter()
            .map(PersistedHardwareGraphDenial::from)
            .collect(),
    };
    let encoded = serde_json::to_string(&state).map_err(HardwareGraphDenialError::Encode)?;
    upsert_config(pool, &encoded).await
}

async fn load_config(pool: &DbPool) -> Result<Option<String>, HardwareGraphDenialError> {
    let row = sqlx::query(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(CONFIG_KEY_HARDWARE_GRAPH_DENIALS)
        .fetch_optional(pool)
        .await?;
    Ok(row
        .map(|row| {
            row.try_get::<Option<String>, _>("Value")
                .map(Option::unwrap_or_default)
        })
        .transpose()?
        .filter(|value| !value.trim().is_empty()))
}

async fn upsert_config(pool: &DbPool, value: &str) -> Result<(), HardwareGraphDenialError> {
    // `REPLACE INTO` is the repository-wide ConfigElement upsert: SQLite's
    // `ON CONFLICT ... excluded` is not valid on MySQL, and this row is written
    // during startup and on every FFmpeg settings save, so a backend-specific
    // statement here would break both on MySQL.
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(CONFIG_KEY_HARDWARE_GRAPH_DENIALS)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_kinds_round_trip_through_persistence() {
        for kind in [
            GraphFailureKind::VulkanDevice,
            GraphFailureKind::Libplacebo,
            GraphFailureKind::HardwareMap,
            GraphFailureKind::CudaInterop,
            GraphFailureKind::EncoderInit,
            GraphFailureKind::MissingFilter,
        ] {
            assert_eq!(failure_kind_from_name(failure_kind_name(kind)), Some(kind));
        }
        assert_eq!(failure_kind_from_name("nonsense"), None);
    }

    #[test]
    fn denials_round_trip_through_serialization() {
        let denial = HardwareGraphDenial::new(
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            "NVIDIA GeForce RTX 3080 Ti",
            GraphFailureKind::CudaInterop,
        );
        let state = PersistedHardwareGraphState {
            ffmpeg_path: Some("/opt/ffmpeg/bin/ffmpeg".to_owned()),
            device: Some("NVIDIA GeForce RTX 3080 Ti".to_owned()),
            proven: Some(vec![HardwareGraphOperation::NvencCudaScale.id().to_owned()]),
            attempted: Some(vec![
                HardwareGraphOperation::NvencHdrVulkanLibplacebo
                    .id()
                    .to_owned(),
                HardwareGraphOperation::NvencCudaScale.id().to_owned(),
            ]),
            denials: vec![PersistedHardwareGraphDenial::from(&denial)],
        };

        let encoded = serde_json::to_string(&state).expect("encode");
        let decoded: PersistedHardwareGraphState = serde_json::from_str(&encoded).expect("decode");

        assert_eq!(
            decoded.ffmpeg_path.as_deref(),
            Some("/opt/ffmpeg/bin/ffmpeg")
        );
        assert_eq!(
            decoded.denials[0].clone().into_denial(),
            Some(denial),
            "the persisted row must reconstruct the exact denial"
        );
    }

    #[test]
    fn malformed_rows_are_ignored_rather_than_breaking_startup() {
        let decoded: PersistedHardwareGraphState =
            serde_json::from_str("this is not json").unwrap_or_default();
        assert!(decoded.denials.is_empty());

        let unknown = PersistedHardwareGraphDenial {
            operation: "no-such-operation".to_owned(),
            device: String::new(),
            kind: "cuda-interop".to_owned(),
        };
        assert_eq!(unknown.into_denial(), None);
    }

    /// Real-hardware gate. Ignored by default because it needs an `ffmpeg`
    /// binary and a working NVIDIA stack; CI machines have neither.
    ///
    /// Run with:
    /// `ERSATZRS_FFMPEG=$HOME/.local/ffmpeg-9.0/bin/ffmpeg \
    ///  ./run.sh test -p ersatzrs-infra --run-ignored ignored-only hardware_graph`
    ///
    /// This is the probe that separates "advertised" from "works here". On the
    /// maintainer's WSL2 host (RTX 3080 Ti, driver 610.47) it denies
    /// `nvenc-hdr-vulkan-libplacebo` with `cuda-interop` — the only Vulkan
    /// device is llvmpipe — and proves `nvenc-cuda-scale`.
    // Test-only opt-in path: the configuration layer is for the running
    // application, and this gate has to point at an arbitrary local binary.
    #[allow(clippy::disallowed_methods)]
    #[tokio::test]
    #[ignore = "requires a real ffmpeg binary and NVIDIA hardware"]
    async fn real_hardware_graph_probe_reports_what_this_machine_can_do() {
        let Some(ffmpeg) = std::env::var_os("ERSATZRS_FFMPEG") else {
            eprintln!("set ERSATZRS_FFMPEG to run this gate");
            return;
        };
        let ffmpeg = Path::new(&ffmpeg);

        for operation in [
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            HardwareGraphOperation::NvencCudaScale,
        ] {
            match probe_hardware_graph(ffmpeg, operation, "local").await {
                HardwareGraphProbe::Proven => println!("PROVEN   {}", operation.id()),
                HardwareGraphProbe::Denied(denial) => {
                    println!("DENIED   {}", denial.evidence());
                }
                HardwareGraphProbe::Unproven => println!("UNPROVEN {}", operation.id()),
            }
        }

        // The fallback graph is the one the retry depends on. If it is denied
        // here there is no hardware path at all, which the operator must see.
        let fallback =
            probe_hardware_graph(ffmpeg, HardwareGraphOperation::NvencCudaScale, "local").await;
        assert!(
            fallback.is_proven(),
            "the CUDA fallback graph must work on a machine with NVENC: {fallback:?}"
        );
    }

    /// Write a fake `ffmpeg` that exits with `code` after printing `stderr`.
    ///
    /// The probe only cares about exit status and the stderr signature, so a
    /// shell script is a faithful stand-in and keeps these tests hermetic —
    /// they assert the classifier and the outcome shape, not `FFmpeg` itself.
    #[cfg(unix)]
    fn write_fake_ffmpeg(dir: &Path, code: i32, stderr: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt as _;

        let path = dir.join("fake-ffmpeg");
        let body = format!("#!/bin/sh\ncat >&2 <<'EOF'\n{stderr}\nEOF\nexit {code}\n");
        std::fs::write(&path, body).expect("write fake ffmpeg");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod");
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_successful_probe_is_the_only_positive_evidence() {
        let dir = tempfile::tempdir().expect("temp dir");
        let ffmpeg = write_fake_ffmpeg(dir.path(), 0, "");

        let outcome =
            probe_hardware_graph(&ffmpeg, HardwareGraphOperation::NvencCudaScale, "gpu").await;

        assert_eq!(outcome, HardwareGraphProbe::Proven);
        assert!(outcome.is_proven());
        assert_eq!(outcome.denial(), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_classified_failure_is_denied_and_persistable() {
        let dir = tempfile::tempdir().expect("temp dir");
        // The measured WSL2 signature.
        let ffmpeg = write_fake_ffmpeg(dir.path(), 1, "[CUDA @ 0x1] Could not derive CUDA device.");

        let outcome = probe_hardware_graph(
            &ffmpeg,
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            "gpu",
        )
        .await;

        assert!(!outcome.is_proven());
        let denial = outcome.denial().expect("a classified failure is a denial");
        assert_eq!(denial.kind, GraphFailureKind::CudaInterop);
        assert_eq!(denial.device, "gpu");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_unclassified_failure_is_unproven_and_not_persisted() {
        // ADR 0014 forbids learning a denial from an arbitrary FFmpeg failure:
        // a full disk must not permanently disable a working graph. But it is
        // also not evidence the graph works, so it must not be selectable.
        let dir = tempfile::tempdir().expect("temp dir");
        let ffmpeg = write_fake_ffmpeg(dir.path(), 1, "No space left on device");

        let outcome =
            probe_hardware_graph(&ffmpeg, HardwareGraphOperation::NvencCudaScale, "gpu").await;

        assert_eq!(outcome, HardwareGraphProbe::Unproven);
        assert!(!outcome.is_proven());
        assert_eq!(outcome.denial(), None, "nothing to persist");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn wholly_unproven_refresh_records_every_probe_attempt() {
        let dir = tempfile::tempdir().expect("temp dir");
        let ffmpeg = write_fake_ffmpeg(dir.path(), 1, "No space left on device");
        let mut report = FfmpegCapabilityReport {
            hwaccels: ["cuda".to_owned(), "vulkan".to_owned()]
                .into_iter()
                .collect(),
            filters: ["libplacebo".to_owned()].into_iter().collect(),
            ..FfmpegCapabilityReport::default()
        };

        refresh_hardware_graph_capabilities(&ffmpeg, &mut report).await;

        assert!(report.has_probed_hardware_graphs());
        assert!(report
            .hardware_graph_probe_attempted(HardwareGraphOperation::NvencHdrVulkanLibplacebo, ""));
        assert!(report.hardware_graph_probe_attempted(HardwareGraphOperation::NvencCudaScale, ""));
        assert!(report.proven_hardware_graphs.is_empty());
        assert!(report.hardware_graph_denials.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_spawn_failure_is_unproven_and_not_persisted() {
        let outcome = probe_hardware_graph(
            Path::new("/nonexistent/ffmpeg-does-not-exist"),
            HardwareGraphOperation::NvencCudaScale,
            "gpu",
        )
        .await;

        assert_eq!(outcome, HardwareGraphProbe::Unproven);
        assert_eq!(outcome.denial(), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_probe_that_hangs_is_denied_rather_than_blocking_startup() {
        // A hung driver is exactly what the probe exists to catch, and it
        // recurs, so unlike the other failures a timeout *is* persistable.
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("hanging-ffmpeg");
        std::fs::write(&path, "#!/bin/sh\nsleep 600\n").expect("write script");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod");

        let outcome = tokio::time::timeout(
            Duration::from_secs(PROBE_TIMEOUT.as_secs() + 10),
            probe_hardware_graph(&path, HardwareGraphOperation::NvencCudaScale, "gpu"),
        )
        .await
        .expect("the probe must bound itself, not hang startup");

        assert!(!outcome.is_proven());
        assert!(outcome.denial().is_some(), "a timeout is persistable");
    }

    // Only used by the SQLite-backed lifecycle tests below.
    #[cfg(not(feature = "mysql"))]
    async fn test_pool() -> DbPool {
        let dir = std::env::temp_dir().join(format!("ersatzrs-graph-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp db dir");
        crate::db::init(&format!("sqlite:{}", dir.join("ErsatzRS.db").display()))
            .await
            .expect("initialize test database")
    }

    // Only used by the SQLite-backed lifecycle tests below.
    #[cfg(not(feature = "mysql"))]
    fn denial() -> HardwareGraphDenial {
        HardwareGraphDenial::new(
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            "NVIDIA GeForce RTX 3080 Ti",
            GraphFailureKind::CudaInterop,
        )
    }

    /// A concurrent worker can still hold the startup snapshot after another
    /// worker has published its denial. Persisting the stale snapshot must not
    /// erase the first denial from the restart state.
    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn stale_worker_snapshot_persists_shared_denials_for_restart() {
        let pool = test_pool().await;
        let gpu = "NVIDIA GeForce RTX 3080 Ti";
        let mut initial = FfmpegCapabilityReport {
            hwaccels: ["cuda".to_owned(), "vulkan".to_owned()]
                .into_iter()
                .collect(),
            filters: ["libplacebo".to_owned()].into_iter().collect(),
            nvidia_gpu_list: Some(format!("[ GPU #0 - < {gpu} > has Compute SM 8.6 ]")),
            ..FfmpegCapabilityReport::default()
        };
        initial.prove_hardware_graph(HardwareGraphOperation::NvencHdrVulkanLibplacebo, gpu);
        initial.prove_hardware_graph(HardwareGraphOperation::NvencCudaScale, gpu);

        let first_denial = denial();
        let mut shared_report = FfmpegRuntimeReport::for_tests();
        shared_report.capabilities = initial.clone();
        shared_report
            .capabilities
            .deny_hardware_graph(first_denial.clone());
        persist_hardware_graph_denials(&pool, "/opt/ffmpeg", &shared_report.capabilities)
            .await
            .expect("persist first worker denial");
        let shared_runtime = RwLock::new(shared_report);

        // This worker started before `first_denial` was learned, so its local
        // snapshot contains only startup proof when it learns another denial.
        let mut stale_worker = initial.clone();
        let second_denial = HardwareGraphDenial::new(
            HardwareGraphOperation::NvencCudaScale,
            gpu,
            GraphFailureKind::HardwareMap,
        );
        persist_hardware_graph_denial_into_runtime(
            &pool,
            "/opt/ffmpeg",
            &mut stale_worker,
            &shared_runtime,
            second_denial.clone(),
        )
        .await
        .expect("persist merged worker denials");

        assert!(stale_worker
            .hardware_graph_denied(HardwareGraphOperation::NvencHdrVulkanLibplacebo, gpu));
        assert!(stale_worker.hardware_graph_denied(HardwareGraphOperation::NvencCudaScale, gpu));
        {
            let shared = shared_runtime.read().await;
            assert!(shared
                .capabilities
                .hardware_graph_denied(HardwareGraphOperation::NvencHdrVulkanLibplacebo, gpu));
            assert!(shared
                .capabilities
                .hardware_graph_denied(HardwareGraphOperation::NvencCudaScale, gpu));
        }

        let persisted = load_hardware_graph_denials(&pool)
            .await
            .expect("reload persisted denials");
        assert_eq!(persisted.len(), 2);
        assert!(persisted.contains(&first_denial));
        assert!(persisted.contains(&second_denial));

        let mut restarted = initial;
        assert!(
            apply_hardware_graph_denials(&pool, "/opt/ffmpeg", &mut restarted)
                .await
                .expect("apply persisted denials after restart")
        );
        assert!(
            restarted.hardware_graph_denied(HardwareGraphOperation::NvencHdrVulkanLibplacebo, gpu)
        );
        assert!(restarted.hardware_graph_denied(HardwareGraphOperation::NvencCudaScale, gpu));
    }

    /// The full persist/overwrite/clear/reload lifecycle on `SQLite`.
    ///
    /// This row is written at startup and on every `FFmpeg` settings save, so a
    /// backend-specific statement here breaks both. The `MySQL` half of this
    /// lifecycle lives in `tests/mysql_backend.rs`.
    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn graph_state_persists_overwrites_clears_and_reloads() {
        let pool = test_pool().await;
        let mut report = FfmpegCapabilityReport::default();

        // Nothing stored yet.
        assert!(load_hardware_graph_denials(&pool)
            .await
            .expect("load empty")
            .is_empty());

        // Persist.
        report.deny_hardware_graph(denial());
        persist_hardware_graph_denials(&pool, "/opt/ffmpeg", &report)
            .await
            .expect("persist");
        assert_eq!(
            load_hardware_graph_denials(&pool).await.expect("reload"),
            vec![denial()]
        );

        // Overwrite: the same key is rewritten, not appended to.
        let mut second = FfmpegCapabilityReport::default();
        second.deny_hardware_graph(HardwareGraphDenial::new(
            HardwareGraphOperation::NvencCudaScale,
            "NVIDIA GeForce RTX 3080 Ti",
            GraphFailureKind::EncoderInit,
        ));
        persist_hardware_graph_denials(&pool, "/opt/ffmpeg", &second)
            .await
            .expect("overwrite");
        let reloaded = load_hardware_graph_denials(&pool).await.expect("reload");
        assert_eq!(reloaded.len(), 1, "overwrite must replace, not append");
        assert_eq!(
            reloaded[0].operation,
            HardwareGraphOperation::NvencCudaScale
        );

        // Startup reload applies the stored rows to a fresh report.
        let mut startup = FfmpegCapabilityReport::default();
        apply_hardware_graph_denials(&pool, "/opt/ffmpeg", &mut startup)
            .await
            .expect("apply");
        assert_eq!(startup.hardware_graph_denials.len(), 1);

        // Clear.
        clear_hardware_graph_denials(&pool).await.expect("clear");
        assert!(load_hardware_graph_denials(&pool)
            .await
            .expect("load cleared")
            .is_empty());
    }

    /// ADR 0014: denials are keyed to the binary they were learned against.
    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn a_changed_ffmpeg_path_clears_stored_denials_at_startup() {
        let pool = test_pool().await;
        let mut report = FfmpegCapabilityReport::default();
        report.deny_hardware_graph(denial());
        persist_hardware_graph_denials(&pool, "/opt/ffmpeg-9.0", &report)
            .await
            .expect("persist");

        let mut startup = FfmpegCapabilityReport::default();
        apply_hardware_graph_denials(&pool, "/opt/ffmpeg-9.1", &mut startup)
            .await
            .expect("apply after upgrade");

        assert!(
            startup.hardware_graph_denials.is_empty(),
            "a different binary must re-probe rather than inherit a denial"
        );
        assert!(
            load_hardware_graph_denials(&pool)
                .await
                .expect("reload")
                .is_empty(),
            "the stale rows are cleared, not just ignored"
        );
    }

    /// The retry ladder both stream supervisors follow, exercised end to end
    /// against the persistence layer.
    ///
    /// The supervisors themselves need a live child process and a duplex
    /// stream, so the steps they perform are driven here instead: classify the
    /// failing stderr, persist a device-scoped denial, and confirm the reloaded
    /// state is what a rebuilt command would select from.
    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn a_runtime_graph_failure_is_classified_persisted_and_reloadable() {
        let pool = test_pool().await;
        let mut report = FfmpegCapabilityReport {
            nvidia_gpu_list: Some(
                "[ GPU #0 - < NVIDIA GeForce RTX 3080 Ti > has Compute SM 8.6 ]".to_owned(),
            ),
            ..FfmpegCapabilityReport::default()
        };
        // Startup probed both graphs and proved them.
        report.prove_hardware_graph(
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            "NVIDIA GeForce RTX 3080 Ti",
        );
        report.prove_hardware_graph(
            HardwareGraphOperation::NvencCudaScale,
            "NVIDIA GeForce RTX 3080 Ti",
        );

        // What a live transcode dies with when the Vulkan device cannot bridge
        // to CUDA, captured from FFmpeg 9.
        let stderr = "[CUDA @ 0x1] Could not derive CUDA device.";
        let kind = GraphFailureKind::classify_runtime(
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            stderr,
        )
        .expect("a known runtime signature");
        let device = report.hardware_graph_device();
        assert_eq!(device, "NVIDIA GeForce RTX 3080 Ti");

        let mut shared_report = FfmpegRuntimeReport::for_tests();
        shared_report.capabilities = report.clone();
        let shared_runtime = RwLock::new(shared_report);
        persist_hardware_graph_denial_into_runtime(
            &pool,
            "/opt/ffmpeg",
            &mut report,
            &shared_runtime,
            HardwareGraphDenial::new(
                HardwareGraphOperation::NvencHdrVulkanLibplacebo,
                &device,
                kind,
            ),
        )
        .await
        .expect("persist the runtime-learned denial");

        // The in-memory report the rebuild sees now denies the graph.
        assert!(
            report.hardware_graph_denied(HardwareGraphOperation::NvencHdrVulkanLibplacebo, &device)
        );

        // And it survives a restart, so the next session does not retry a
        // known-broken graph. The restarted report carries the same runtime
        // identity, as a real restart on the same machine would.
        let mut next_session = FfmpegCapabilityReport {
            nvidia_gpu_list: report.nvidia_gpu_list.clone(),
            hwaccels: ["cuda".to_owned(), "vulkan".to_owned()]
                .into_iter()
                .collect(),
            filters: ["libplacebo".to_owned()].into_iter().collect(),
            ..FfmpegCapabilityReport::default()
        };
        apply_hardware_graph_denials(&pool, "/opt/ffmpeg", &mut next_session)
            .await
            .expect("reload after restart");
        assert!(next_session
            .hardware_graph_denied(HardwareGraphOperation::NvencHdrVulkanLibplacebo, &device));
    }

    /// An arbitrary runtime failure must stay terminal.
    #[test]
    fn an_unrecognised_runtime_failure_produces_no_denial_and_no_retry() {
        // The supervisors only enter the retry ladder when `classify_runtime`
        // returns
        // Some, so an unclassified failure never learns a denial and never
        // respawns — it fails the stream, as ADR 0014 requires.
        for stderr in [
            "No space left on device",
            "Connection timed out",
            "Invalid data found when processing input",
            "Error reinitializing filters!",
            "Failed to configure output pad on Parsed_overlay_3",
            "Error while opening encoder for output stream #0:0",
        ] {
            assert_eq!(
                GraphFailureKind::classify_runtime(HardwareGraphOperation::NvencCudaScale, stderr),
                None,
                "{stderr}"
            );
        }
    }

    #[cfg(not(feature = "mysql"))]
    fn probed_report(gpu: &str) -> FfmpegCapabilityReport {
        FfmpegCapabilityReport {
            nvidia_gpu_list: Some(format!("[ GPU #0 - < {gpu} > has Compute SM 8.6 ]")),
            hwaccels: ["cuda".to_owned(), "vulkan".to_owned()]
                .into_iter()
                .collect(),
            filters: ["libplacebo".to_owned()].into_iter().collect(),
            ..FfmpegCapabilityReport::default()
        }
    }

    /// A record is reusable only with matching identities and terminal evidence
    /// for every applicable operation.
    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn only_a_complete_identity_matching_record_is_reused() {
        let gpu = "NVIDIA GeForce RTX 3080 Ti";

        // A fully successful probe records zero denials. That must still count
        // as a valid cache — the bug this guards against is treating an empty
        // denial list as "never probed" and re-probing forever, or worse, as
        // "everything works" when nothing ran.
        let pool = test_pool().await;
        let mut complete = probed_report(gpu);
        complete.prove_hardware_graph(HardwareGraphOperation::NvencHdrVulkanLibplacebo, gpu);
        complete.prove_hardware_graph(HardwareGraphOperation::NvencCudaScale, gpu);
        persist_hardware_graph_denials(&pool, "/opt/ffmpeg", &complete)
            .await
            .expect("persist");

        let mut reload = probed_report(gpu);
        assert!(
            apply_hardware_graph_denials(&pool, "/opt/ffmpeg", &mut reload)
                .await
                .expect("apply"),
            "success with zero denials is a valid cache"
        );
        assert!(reload.hardware_graph_proven(HardwareGraphOperation::NvencCudaScale, gpu));

        // A partial record: one operation returned Unproven, so it has neither
        // proof nor denial and the record must not be reused.
        let pool = test_pool().await;
        let mut partial = probed_report(gpu);
        partial.prove_hardware_graph(HardwareGraphOperation::NvencCudaScale, gpu);
        persist_hardware_graph_denials(&pool, "/opt/ffmpeg", &partial)
            .await
            .expect("persist");
        let mut reload = probed_report(gpu);
        assert!(
            !apply_hardware_graph_denials(&pool, "/opt/ffmpeg", &mut reload)
                .await
                .expect("apply"),
            "a partial record must re-probe"
        );

        // A different GPU at the same ffmpeg path.
        let pool = test_pool().await;
        persist_hardware_graph_denials(&pool, "/opt/ffmpeg", &complete)
            .await
            .expect("persist");
        let mut other_gpu = probed_report("NVIDIA GeForce RTX 4090");
        assert!(
            !apply_hardware_graph_denials(&pool, "/opt/ffmpeg", &mut other_gpu)
                .await
                .expect("apply"),
            "a GPU swap must re-probe"
        );

        // FFmpeg replaced at the same path.
        let pool = test_pool().await;
        persist_hardware_graph_denials(&pool, "/opt/ffmpeg", &complete)
            .await
            .expect("persist");
        let mut new_binary = probed_report(gpu);
        assert!(
            !apply_hardware_graph_denials(&pool, "/opt/ffmpeg-9.1", &mut new_binary)
                .await
                .expect("apply"),
            "a different ffmpeg must re-probe"
        );
    }

    /// A row written before completion tracking existed cannot be attributed.
    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn a_legacy_record_is_discarded_rather_than_trusted() {
        let pool = test_pool().await;
        // Shape written by the previous implementation: path and denials only.
        upsert_config(
            &pool,
            r#"{"ffmpeg_path":"/opt/ffmpeg","denials":[{"operation":"nvenc-cuda-scale","device":"","kind":"cuda-interop"}]}"#,
        )
        .await
        .expect("seed legacy row");

        let mut report = probed_report("NVIDIA GeForce RTX 3080 Ti");
        assert!(
            !apply_hardware_graph_denials(&pool, "/opt/ffmpeg", &mut report)
                .await
                .expect("apply"),
            "a legacy record carries no completion or device evidence"
        );
        assert!(
            load_hardware_graph_denials(&pool)
                .await
                .expect("reload")
                .is_empty(),
            "and is cleared rather than partially applied"
        );
    }
}
