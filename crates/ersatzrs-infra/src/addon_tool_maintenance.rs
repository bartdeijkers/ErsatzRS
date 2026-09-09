//! Restart-safe startup and daily maintenance for permission-reviewed add-on tools.

use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, TimeDelta, Utc};
use ersatzrs_addon_contract::{
    AddonManifest, ToolUpdateDiagnosticCode, ToolUpdateOutcome, ToolUpdateResult,
    CAPABILITY_TOOL_UPDATE_V1,
};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    AddonToolMaintenanceOutcome, AddonToolMaintenanceState, AddonToolMaintenanceTrigger,
};
use sqlx::Row as _;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::addons::{invoke_tool_update, AddonError, AddonRuntimeContext};
use crate::db::DbPool;
use crate::repositories::common::{encode_datetime, encode_enum};
use crate::repositories::SqlxResultExt as _;

const LEASE_SECONDS: i64 = 120;
const CONTENTION_DELAY_SECONDS: i64 = 60;
const INSTALLATION_PAGE_SIZE: i64 = 100;
// Matches the package-manifest input limit in addons.rs.
const MAX_MANIFEST_BYTES: u32 = 256 * 1024;

/// Durable admission and execution over the existing maintenance history table.
#[derive(Clone)]
pub struct AddonToolMaintenanceCoordinator {
    pool: DbPool,
    startup_scheduled: Arc<Mutex<bool>>,
}

struct Claim {
    id: i64,
    addon_id: String,
    tool_key: String,
    owner: String,
}

impl AddonToolMaintenanceCoordinator {
    /// Create a coordinator for one host boot; clones share startup admission.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self {
            pool,
            startup_scheduled: Arc::new(Mutex::new(false)),
        }
    }

    /// Admit one startup attempt per enabled, approved tool for this host boot.
    /// Queued, running and deferred attempts survive restart and coalesce admission.
    /// # Errors
    /// Returns a persistence error if admission cannot complete.
    pub async fn schedule_startup(&self, now: DateTime<Utc>) -> Result<()> {
        let mut scheduled = self.startup_scheduled.lock().await;
        if !*scheduled {
            self.admit(now, AddonToolMaintenanceTrigger::Startup)
                .await?;
            *scheduled = true;
        }
        Ok(())
    }

    /// Admit daily work and execute at most one owner-fenced maintenance attempt.
    /// Invocation owns the process-tree deadline, cancellation and executable lease.
    /// A busy executable defers this row without accumulating failed attempts.
    /// # Errors
    /// Returns persistence errors; provider failures become redacted durable outcomes.
    pub async fn process_one(
        &self,
        now: DateTime<Utc>,
        mut cancellation: oneshot::Receiver<()>,
    ) -> Result<bool> {
        let started = Instant::now();
        if !matches!(
            cancellation.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ) {
            return Ok(false);
        }
        self.admit(now, AddonToolMaintenanceTrigger::Daily).await?;
        // Admission can wait on other writers and page many installations.
        // Neither that delay nor a shutdown arriving during it may be ignored.
        if !matches!(
            cancellation.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ) {
            return Ok(false);
        }
        let Some(claim) = self.claim(logical_now(now, started)).await? else {
            return Ok(false);
        };
        let outcome = invoke_tool_update(
            &self.pool,
            &claim.addon_id,
            &claim.tool_key,
            &AddonRuntimeContext {
                ffmpeg_path: Some(ersatzrs_ffmpeg::ffmpeg_path()),
                ffprobe_path: Some(ersatzrs_ffmpeg::ffprobe_path()),
            },
            cancellation,
        )
        .await;
        self.record(&claim, logical_now(now, started), outcome)
            .await?;
        Ok(true)
    }

    async fn admit(&self, now: DateTime<Utc>, trigger: AddonToolMaintenanceTrigger) -> Result<()> {
        let mut after = String::new();
        loop {
            // Bound each read; an invalid stored manifest cannot starve later packages.
            let rows = sqlx::query(
                r#"SELECT "AddonId", "ManifestJson" FROM "AddonInstallation"
                   WHERE "Enabled" = 1 AND "AddonId" > ?
                     AND length(CAST("ManifestJson" AS BLOB)) <= ?
                   ORDER BY "AddonId" LIMIT ?"#,
            )
            .bind(&after)
            .bind(i64::from(MAX_MANIFEST_BYTES))
            .bind(INSTALLATION_PAGE_SIZE)
            .fetch_all(&self.pool)
            .await
            .repository()?;
            if rows.is_empty() {
                return Ok(());
            }
            for row in rows {
                let addon_id: String = row.try_get("AddonId").repository()?;
                after.clone_from(&addon_id);
                let json: String = row.try_get("ManifestJson").repository()?;
                let Some(manifest) = approved_manifest(&json) else {
                    continue;
                };
                let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
                for key in &manifest.permissions.tool_update {
                    // Recheck the exact manifest and enabled state inside the write lock.
                    // Daily admission uses completed attempts, including failures/manual
                    // outcomes, so it cannot spin repeatedly after a failed updater.
                    sqlx::query(
                        r#"INSERT INTO "AddonToolMaintenanceJob" (
                               "AddonId", "ToolKey", "Trigger", "State", "CreatedAt", "UpdatedAt"
                           ) SELECT ?, ?, ?, 0, ?, ?
                           WHERE EXISTS (SELECT 1 FROM "AddonInstallation"
                               WHERE "AddonId" = ? AND "Enabled" = 1 AND "ManifestJson" = ?)
                             AND NOT EXISTS (SELECT 1 FROM "AddonToolMaintenanceJob"
                               WHERE "AddonId" = ? AND "ToolKey" = ? AND "State" IN (0, 1, 2))
                             AND (? = 0 OR NOT EXISTS (
                               SELECT 1 FROM "AddonToolMaintenanceJob"
                               WHERE "AddonId" = ? AND "ToolKey" = ?
                                 AND julianday("CompletedAt") > julianday(?)))"#,
                    )
                    .bind(&addon_id)
                    .bind(key)
                    .bind(encode_enum(trigger))
                    .bind(encode_datetime(now))
                    .bind(encode_datetime(now))
                    .bind(&addon_id)
                    .bind(&json)
                    .bind(&addon_id)
                    .bind(key)
                    .bind(encode_enum(trigger))
                    .bind(&addon_id)
                    .bind(key)
                    .bind(encode_datetime(now - TimeDelta::hours(24)))
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                }
                tx.commit().await.repository()?;
            }
        }
    }

    async fn claim(&self, now: DateTime<Utc>) -> Result<Option<Claim>> {
        let started = Instant::now();
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let now = logical_now(now, started);
        let now_text = encode_datetime(now);
        let row = sqlx::query(
            r#"SELECT "job"."Id", "job"."AddonId", "job"."ToolKey"
               FROM "AddonToolMaintenanceJob" AS "job"
               WHERE (("job"."State" IN (0, 2)
                         AND ("job"."NextEligibleAt" IS NULL
                              OR julianday("job"."NextEligibleAt") <= julianday(?)))
                      OR ("job"."State" = 1
                         AND ("job"."LeaseExpiresAt" IS NULL
                              OR julianday("job"."LeaseExpiresAt") <= julianday(?))))
                 AND NOT EXISTS (SELECT 1 FROM "AddonToolMaintenanceJob" AS "other"
                     WHERE "other"."AddonId" = "job"."AddonId"
                       AND "other"."ToolKey" = "job"."ToolKey"
                       AND "other"."State" = 1 AND "other"."Id" != "job"."Id")
               ORDER BY "job"."CreatedAt", "job"."Id" LIMIT 1"#,
        )
        .bind(&now_text)
        .bind(&now_text)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let Some(row) = row else {
            tx.commit().await.repository()?;
            return Ok(None);
        };
        let claim = Claim {
            id: row.try_get("Id").repository()?,
            addon_id: row.try_get("AddonId").repository()?,
            tool_key: row.try_get("ToolKey").repository()?,
            owner: Uuid::new_v4().to_string(),
        };
        sqlx::query(
            r#"UPDATE "AddonToolMaintenanceJob"
               SET "State" = 1, "LeaseOwner" = ?, "LeaseExpiresAt" = ?,
                   "HeartbeatAt" = ?, "UpdatedAt" = ?, "NextEligibleAt" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(&claim.owner)
        .bind(encode_datetime(now + TimeDelta::seconds(LEASE_SECONDS)))
        .bind(&now_text)
        .bind(&now_text)
        .bind(claim.id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(Some(claim))
    }

    async fn record(
        &self,
        claim: &Claim,
        now: DateTime<Utc>,
        result: std::result::Result<ToolUpdateResult, AddonError>,
    ) -> Result<()> {
        match result {
            Err(AddonError::ToolBusy) => self.defer(claim, now, "tool-busy").await,
            Err(AddonError::ToolCleanupRequired | AddonError::TerminationUnconfirmed) => {
                self.defer(claim, now, "tool-cleanup-required").await
            }
            Ok(response) if response.validate().is_ok() && response.tool_key == claim.tool_key => {
                self.complete(claim, now, Some(&response)).await
            }
            // Never persist AddonError Display: process and configuration errors
            // may carry executable paths or provider output.
            Err(_) | Ok(_) => self.complete(claim, now, None).await,
        }
    }

    async fn defer(&self, claim: &Claim, now: DateTime<Utc>, code: &str) -> Result<()> {
        let changed = sqlx::query(
            r#"UPDATE "AddonToolMaintenanceJob"
               SET "State" = 2, "NextEligibleAt" = ?, "LeaseOwner" = NULL,
                   "LeaseExpiresAt" = NULL, "HeartbeatAt" = NULL,
                   "DiagnosticCode" = ?, "Diagnostic" = NULL, "UpdatedAt" = ?
               WHERE "Id" = ? AND "State" = 1 AND "LeaseOwner" = ?
                 AND julianday("LeaseExpiresAt") > julianday(?)"#,
        )
        .bind(encode_datetime(
            now + TimeDelta::seconds(CONTENTION_DELAY_SECONDS),
        ))
        .bind(code)
        .bind(encode_datetime(now))
        .bind(claim.id)
        .bind(&claim.owner)
        .bind(encode_datetime(now))
        .execute(&self.pool)
        .await
        .repository()?;
        require_owner(changed.rows_affected())
    }

    async fn complete(
        &self,
        claim: &Claim,
        now: DateTime<Utc>,
        response: Option<&ToolUpdateResult>,
    ) -> Result<()> {
        let outcome = match response.map(|result| result.outcome) {
            Some(ToolUpdateOutcome::Updated) => AddonToolMaintenanceOutcome::Updated,
            Some(ToolUpdateOutcome::AlreadyCurrent) => AddonToolMaintenanceOutcome::AlreadyCurrent,
            Some(ToolUpdateOutcome::ManualUpdateRequired) => {
                AddonToolMaintenanceOutcome::ManualUpdateRequired
            }
            Some(ToolUpdateOutcome::Failed) | None => AddonToolMaintenanceOutcome::Failed,
        };
        let state = if outcome == AddonToolMaintenanceOutcome::Failed {
            AddonToolMaintenanceState::Failed
        } else {
            AddonToolMaintenanceState::Completed
        };
        let diagnostic = response.map_or(Some("tool-update-failed"), |result| {
            result.diagnostic_code.map(diagnostic_code)
        });
        let changed = sqlx::query(
            r#"UPDATE "AddonToolMaintenanceJob"
               SET "State" = ?, "Outcome" = ?, "BeforeVersion" = ?, "AfterVersion" = ?,
                   "DiagnosticCode" = ?, "Diagnostic" = NULL,
                   "AttemptCount" = MIN("AttemptCount" + 1, 2147483647),
                   "NextEligibleAt" = NULL, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "HeartbeatAt" = NULL, "UpdatedAt" = ?, "CompletedAt" = ?
               WHERE "Id" = ? AND "State" = 1 AND "LeaseOwner" = ?
                 AND julianday("LeaseExpiresAt") > julianday(?)"#,
        )
        .bind(encode_enum(state))
        .bind(encode_enum(outcome))
        .bind(response.and_then(|result| result.before_version.as_deref()))
        .bind(response.and_then(|result| result.after_version.as_deref()))
        .bind(diagnostic)
        .bind(encode_datetime(now))
        .bind(encode_datetime(now))
        .bind(claim.id)
        .bind(&claim.owner)
        .bind(encode_datetime(now))
        .execute(&self.pool)
        .await
        .repository()?;
        require_owner(changed.rows_affected())
    }
}

fn approved_manifest(json: &str) -> Option<AddonManifest> {
    if json.len() > MAX_MANIFEST_BYTES as usize {
        return None;
    }
    let manifest: AddonManifest = serde_json::from_str(json).ok()?;
    (manifest.validate().is_ok() && manifest.has_capability(CAPABILITY_TOOL_UPDATE_V1))
        .then_some(manifest)
}

fn logical_now(origin: DateTime<Utc>, started: Instant) -> DateTime<Utc> {
    origin
        + TimeDelta::from_std(started.elapsed())
            .unwrap_or_else(|_| TimeDelta::seconds(LEASE_SECONDS))
}

fn diagnostic_code(code: ToolUpdateDiagnosticCode) -> &'static str {
    match code {
        ToolUpdateDiagnosticCode::UnsupportedInstallation => "unsupported_installation",
        ToolUpdateDiagnosticCode::UpdateFailed => "update_failed",
        ToolUpdateDiagnosticCode::VersionProbeFailed => "version_probe_failed",
        ToolUpdateDiagnosticCode::ToolUnavailable => "tool_unavailable",
    }
}

fn require_owner(affected: u64) -> Result<()> {
    if affected == 1 {
        Ok(())
    } else {
        Err(RepositoryError::Conflict(
            "tool maintenance lease is no longer owned by this worker".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn logical_clock_includes_admission_delay() {
        let started = Instant::now()
            .checked_sub(std::time::Duration::from_mins(3))
            .unwrap();
        assert!(logical_now(now(), started) >= now() + TimeDelta::seconds(180));
    }

    fn manifest() -> serde_json::Value {
        serde_json::json!({
            "manifest_version": 6,
            "id": "org.ersatzrs.addon.fixture",
            "version": "1.0.0",
            "name": {"en-US": "Fixture"},
            "summary": {"en-US": "Tool maintenance fixture"},
            "license": "MIT",
            "source_url": "https://example.test/source",
            "host_version": "*",
            "capabilities": [{"id": "addon.check.v1"}, {"id": "tool.update.v1"}],
            "entrypoints": {"any": {"path": "bin/tool", "kind": "native"}},
            "permissions": {"tool_update": ["TOOL_BIN"]},
            "settings": [{"key": "TOOL_BIN", "label": {"en-US": "Tool"},
                          "kind": "executable", "default": "tool"}]
        })
    }

    async fn coordinator() -> AddonToolMaintenanceCoordinator {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            r#"CREATE TABLE "AddonInstallation" (
                   "AddonId" TEXT PRIMARY KEY, "Enabled" INTEGER NOT NULL,
                   "ManifestJson" TEXT NOT NULL)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        // Exercise the canonical production table and indexes without unrelated seeds.
        let (_, table) =
            include_str!("../../../migrations/202609041500_addon_list_lifecycle_foundation.sql")
                .split_once("CREATE TABLE \"AddonToolMaintenanceJob\"")
                .unwrap();
        // Only checked-in migration text is composed here, never fixture input.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE TABLE \"AddonToolMaintenanceJob\"{table}"
        )))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" ("AddonId", "Enabled", "ManifestJson")
               VALUES ('org.ersatzrs.addon.fixture', 1, ?)"#,
        )
        .bind(manifest().to_string())
        .execute(&pool)
        .await
        .unwrap();
        AddonToolMaintenanceCoordinator::new(pool)
    }

    async fn count(coordinator: &AddonToolMaintenanceCoordinator) -> i64 {
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonToolMaintenanceJob""#)
            .fetch_one(&coordinator.pool)
            .await
            .unwrap()
    }

    fn success(claim: &Claim) -> ToolUpdateResult {
        ToolUpdateResult {
            schema: CAPABILITY_TOOL_UPDATE_V1.to_owned(),
            tool_key: claim.tool_key.clone(),
            before_version: Some("1.0".to_owned()),
            after_version: Some("1.1".to_owned()),
            outcome: ToolUpdateOutcome::Updated,
            diagnostic_code: None,
        }
    }

    #[tokio::test]
    async fn repeated_startup_coalesces_pending_and_each_new_boot_admits_once() {
        let coordinator = coordinator().await;
        let other = AddonToolMaintenanceCoordinator::new(coordinator.pool.clone());
        let (first, second) = tokio::join!(
            coordinator.schedule_startup(now()),
            other.schedule_startup(now())
        );
        first.unwrap();
        second.unwrap();
        assert_eq!(count(&coordinator).await, 1);
        let claim = coordinator.claim(now()).await.unwrap().unwrap();
        coordinator
            .record(&claim, now(), Ok(success(&claim)))
            .await
            .unwrap();
        coordinator.schedule_startup(now()).await.unwrap();
        assert_eq!(count(&coordinator).await, 1);
        let reboot = AddonToolMaintenanceCoordinator::new(coordinator.pool.clone());
        reboot.schedule_startup(now()).await.unwrap();
        assert_eq!(count(&coordinator).await, 2);
    }

    #[tokio::test]
    async fn daily_admission_waits_full_day_after_completion_including_failure() {
        let coordinator = coordinator().await;
        coordinator.schedule_startup(now()).await.unwrap();
        let claim = coordinator.claim(now()).await.unwrap().unwrap();
        coordinator.complete(&claim, now(), None).await.unwrap();
        coordinator
            .admit(
                now() + TimeDelta::hours(24) - TimeDelta::seconds(1),
                AddonToolMaintenanceTrigger::Daily,
            )
            .await
            .unwrap();
        assert_eq!(count(&coordinator).await, 1);
        coordinator
            .admit(
                now() + TimeDelta::hours(24),
                AddonToolMaintenanceTrigger::Daily,
            )
            .await
            .unwrap();
        assert_eq!(count(&coordinator).await, 2);
    }

    #[tokio::test]
    async fn concurrent_claims_have_one_winner_and_stale_completion_is_fenced() {
        let coordinator = coordinator().await;
        coordinator.schedule_startup(now()).await.unwrap();
        let (first, second) = tokio::join!(coordinator.claim(now()), coordinator.claim(now()));
        let claims: Vec<_> = [first.unwrap(), second.unwrap()]
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(claims.len(), 1);
        let old = &claims[0];
        let later = now() + TimeDelta::seconds(LEASE_SECONDS + 1);
        let replacement = coordinator.claim(later).await.unwrap().unwrap();
        assert_eq!(old.id, replacement.id);
        assert_ne!(old.owner, replacement.owner);
        assert!(coordinator
            .record(old, later, Ok(success(old)))
            .await
            .is_err());
        coordinator
            .record(&replacement, later, Ok(success(&replacement)))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn contention_defers_same_attempt_without_failure_or_daily_bypass() {
        let coordinator = coordinator().await;
        coordinator.schedule_startup(now()).await.unwrap();
        let claim = coordinator.claim(now()).await.unwrap().unwrap();
        coordinator
            .record(&claim, now(), Err(AddonError::ToolBusy))
            .await
            .unwrap();
        assert!(coordinator
            .claim(now() + TimeDelta::seconds(59))
            .await
            .unwrap()
            .is_none());
        coordinator
            .admit(
                now() + TimeDelta::hours(25),
                AddonToolMaintenanceTrigger::Daily,
            )
            .await
            .unwrap();
        assert_eq!(count(&coordinator).await, 1);
        let resumed = coordinator
            .claim(now() + TimeDelta::seconds(60))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resumed.id, claim.id);
        coordinator
            .record(
                &resumed,
                now() + TimeDelta::seconds(60),
                Err(AddonError::ToolCleanupRequired),
            )
            .await
            .unwrap();
        let row = sqlx::query(
            r#"SELECT "AttemptCount", "Outcome", "DiagnosticCode" FROM "AddonToolMaintenanceJob""#,
        )
        .fetch_one(&coordinator.pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i32, _>("AttemptCount"), 0);
        assert!(row.get::<Option<i32>, _>("Outcome").is_none());
        assert_eq!(
            row.get::<String, _>("DiagnosticCode"),
            "tool-cleanup-required"
        );
    }

    #[tokio::test]
    async fn disabled_and_unapproved_manifest_never_enter_scheduler() {
        let coordinator = coordinator().await;
        sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 0"#)
            .execute(&coordinator.pool)
            .await
            .unwrap();
        coordinator
            .admit(now(), AddonToolMaintenanceTrigger::Startup)
            .await
            .unwrap();
        assert_eq!(count(&coordinator).await, 0);
        let mut unapproved = manifest();
        unapproved["permissions"] = serde_json::json!({});
        sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 1, "ManifestJson" = ?"#)
            .bind(unapproved.to_string())
            .execute(&coordinator.pool)
            .await
            .unwrap();
        coordinator
            .admit(now(), AddonToolMaintenanceTrigger::Startup)
            .await
            .unwrap();
        assert_eq!(count(&coordinator).await, 0);
    }

    // POSIX shell fixture; Windows uses its separate native package acceptance.
    #[cfg(unix)]
    #[test]
    fn scheduled_update_supplies_declared_managed_ffmpeg_context() {
        let state = tempfile::tempdir().unwrap();
        // Isolate process-global startup configuration without mutating the
        // concurrent test process environment or touching an operator profile.
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "addon_tool_maintenance::tests::scheduled_update_managed_ffmpeg_fixture",
                "--ignored",
            ])
            .env(
                crate::file_system_layout::APP_STATE_DIR_ENV_VAR,
                state.path(),
            )
            .env("ERSATZRS_TOOL_MAINTENANCE_TEST_PROFILE", state.path())
            .status()
            .unwrap();
        assert!(status.success(), "isolated maintenance fixture failed");
    }

    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "subprocess fixture invoked with an isolated application state directory"]
    async fn scheduled_update_managed_ffmpeg_fixture() {
        let isolated_state =
            crate::startup_config::environment_variable("ERSATZRS_TOOL_MAINTENANCE_TEST_PROFILE")
                .expect("fixture requires isolated application state");
        assert_eq!(
            crate::file_system_layout::app_state_folder(),
            std::path::PathBuf::from(isolated_state)
        );
        let temporary = tempfile::tempdir().unwrap();
        let package = temporary.path().join("package");
        std::fs::create_dir(&package).unwrap();
        let tool = temporary.path().join("tool-identity");
        std::fs::write(&tool, b"fixture identity; never executed").unwrap();
        let mut manifest = manifest();
        manifest["permissions"]["managed_ffmpeg"] = serde_json::json!(true);
        manifest["settings"][0]["default"] = serde_json::json!(tool.to_str().unwrap());
        manifest["entrypoints"]["any"] =
            serde_json::json!({"path": "addon.sh", "kind": "posix_shell"});
        let manifest: AddonManifest = serde_json::from_value(manifest).unwrap();
        std::fs::write(
            package.join("addon.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(
            package.join("addon.sh"),
            r#"test "$1" = update || exit 1
test -n "$FFMPEG_BIN" && test -n "$FFPROBE_BIN" || exit 1
read -r request
printf '%s\n' '{"schema":"tool.update.v1","tool_key":"TOOL_BIN","before_version":"1.0","after_version":"1.1","outcome":"updated","diagnostic_code":null}'
"#,
        )
        .unwrap();
        let content_sha256 = crate::addons::calculate_addon_content_sha256(&package)
            .await
            .unwrap();
        let pool = crate::db::init("sqlite::memory:").await.unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "StateRevision"
               ) VALUES (?, '1.0.0', 1, 'fixture-package', ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&manifest.id)
        .bind(content_sha256)
        .bind(serde_json::to_string(&manifest).unwrap())
        .bind(package.to_str().unwrap())
        .bind(encode_datetime(now()))
        .bind(encode_datetime(now()))
        .bind(Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();
        let coordinator = AddonToolMaintenanceCoordinator::new(pool);
        coordinator.schedule_startup(now()).await.unwrap();
        let (_cancel, cancellation) = oneshot::channel();
        assert!(coordinator.process_one(now(), cancellation).await.unwrap());
        let row = sqlx::query(
            r#"SELECT "State", "Outcome", "BeforeVersion", "AfterVersion", "DiagnosticCode"
               FROM "AddonToolMaintenanceJob""#,
        )
        .fetch_one(&coordinator.pool)
        .await
        .unwrap();
        assert_eq!(
            row.get::<i32, _>("State"),
            3,
            "maintenance diagnostic: {:?}",
            row.get::<Option<String>, _>("DiagnosticCode")
        );
        assert_eq!(row.get::<i32, _>("Outcome"), 0);
        assert_eq!(row.get::<String, _>("BeforeVersion"), "1.0");
        assert_eq!(row.get::<String, _>("AfterVersion"), "1.1");
    }
}
