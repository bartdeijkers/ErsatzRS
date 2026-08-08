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
    FfmpegCapabilityReport, GraphFailureKind, HardwareGraphDenial, HardwareGraphOperation,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

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

/// Persisted graph-denial state, keyed by the identities ADR 0014 requires:
/// the `ffmpeg` binary the probe ran against and the device it ran on. The
/// per-row device lives on the denial itself; the binary path is shared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct PersistedHardwareGraphState {
    #[serde(default)]
    ffmpeg_path: Option<String>,
    #[serde(default)]
    denials: Vec<PersistedHardwareGraphDenial>,
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
    if !report.has_hwaccel("cuda") {
        return;
    }
    let device = report.hardware_graph_device();

    let mut operations = Vec::new();
    if report.has_hwaccel("vulkan") && report.filters.contains("libplacebo") {
        operations.push(HardwareGraphOperation::NvencHdrVulkanLibplacebo);
    }
    operations.push(HardwareGraphOperation::NvencCudaScale);

    for operation in operations {
        match probe_hardware_graph(ffmpeg_path, operation, &device).await {
            HardwareGraphProbe::Proven => report.prove_hardware_graph(operation, &device),
            HardwareGraphProbe::Denied(denial) => report.deny_hardware_graph(denial),
            // Left with neither proof nor denial: not selectable now, and not
            // persisted, so the next refresh re-probes.
            HardwareGraphProbe::Unproven => {}
        }
    }
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

/// Apply persisted graph denials to a startup report, clearing them when the
/// `ffmpeg` binary changed since they were learned (ADR 0014 lifecycle).
///
/// # Errors
///
/// Returns an error when the database read or write fails.
pub async fn apply_hardware_graph_denials(
    pool: &DbPool,
    ffmpeg_path: &str,
    report: &mut FfmpegCapabilityReport,
) -> Result<(), HardwareGraphDenialError> {
    let Some(raw) = load_config(pool).await? else {
        return Ok(());
    };
    let state: PersistedHardwareGraphState = serde_json::from_str(&raw).unwrap_or_default();

    if state
        .ffmpeg_path
        .as_deref()
        .is_some_and(|stored| stored != ffmpeg_path)
    {
        tracing::info!("ffmpeg path changed since hardware graph denials were learned; clearing");
        clear_hardware_graph_denials(pool).await?;
        return Ok(());
    }

    for denial in state
        .denials
        .into_iter()
        .filter_map(PersistedHardwareGraphDenial::into_denial)
    {
        report.deny_hardware_graph(denial);
    }
    Ok(())
}

/// Clear persisted graph denials after an explicit capability refresh.
///
/// # Errors
///
/// Returns an error when the database write fails.
pub async fn clear_hardware_graph_denials(pool: &DbPool) -> Result<(), HardwareGraphDenialError> {
    upsert_config(pool, "").await
}

/// Learn one graph denial at runtime and persist it.
///
/// Used by the stream supervisors when a live transcode dies with a recognised
/// graph signature: startup probing cannot catch a graph that only fails under
/// real media, so the denial is learned from the failure itself.
///
/// # Errors
///
/// Returns an error when encoding or the database write fails.
pub async fn persist_hardware_graph_denial(
    pool: &DbPool,
    ffmpeg_path: &str,
    report: &mut FfmpegCapabilityReport,
    denial: HardwareGraphDenial,
) -> Result<(), HardwareGraphDenialError> {
    report.deny_hardware_graph(denial);
    persist_hardware_graph_denials(pool, ffmpeg_path, report).await
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
        report.prove_hardware_graph(
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            "NVIDIA GeForce RTX 3080 Ti",
        );

        // What a live transcode dies with when the Vulkan device cannot bridge
        // to CUDA, captured from FFmpeg 9.
        let stderr = "[CUDA @ 0x1] Could not derive CUDA device.";
        let kind = GraphFailureKind::classify(stderr).expect("a known runtime signature");
        let device = report.hardware_graph_device();
        assert_eq!(device, "NVIDIA GeForce RTX 3080 Ti");

        persist_hardware_graph_denial(
            &pool,
            "/opt/ffmpeg",
            &mut report,
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
        // known-broken graph.
        let mut next_session = FfmpegCapabilityReport::default();
        apply_hardware_graph_denials(&pool, "/opt/ffmpeg", &mut next_session)
            .await
            .expect("reload after restart");
        assert!(next_session
            .hardware_graph_denied(HardwareGraphOperation::NvencHdrVulkanLibplacebo, &device));
    }

    /// An arbitrary runtime failure must stay terminal.
    #[test]
    fn an_unrecognised_runtime_failure_produces_no_denial_and_no_retry() {
        // The supervisors only enter the retry ladder when `classify` returns
        // Some, so an unclassified failure never learns a denial and never
        // respawns — it fails the stream, as ADR 0014 requires.
        for stderr in [
            "No space left on device",
            "Connection timed out",
            "Invalid data found when processing input",
        ] {
            assert_eq!(GraphFailureKind::classify(stderr), None, "{stderr}");
        }
    }
}
