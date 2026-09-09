use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use crate::db::DbRow;
use ersatzrs_addon_contract::{AddonCheckStatus, AddonManifest};
use ersatzrs_core::repositories::AddonMediaListLifecycleRepository;
use semver::Version;

pub(crate) struct AddonPlatformHealthCheck {
    pool: Pool<Db>,
}

impl AddonPlatformHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for AddonPlatformHealthCheck {
    fn title(&self) -> &'static str {
        "Add-ons"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let sources = sqlx::query(
            r#"SELECT "Id", "Kind", "LastError"
               FROM "AddonSource" ORDER BY "Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        let installations = sqlx::query(
            r#"SELECT "AddonId", "SourceId", "RepositoryId", "Enabled", "TrustTier", "ManifestJson", "InstallPath", "ContentSha256", "LastCheckCode", "LastCheckStatus"
               FROM "AddonInstallation" ORDER BY "AddonId""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if installations.is_empty() && sources.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let mut failures = Vec::new();
        let mut warnings = Vec::new();
        for row in sources {
            if let Some(warning) = local_source_finding(&row) {
                warnings.push(warning);
            }
        }

        let host_version = Version::parse(env!("CARGO_PKG_VERSION")).ok();
        let mut usable_default_addons = Vec::new();
        for row in installations {
            warnings.extend(tool_maintenance_findings(&self.pool, &row).await?);
            if default_addon_is_usable(&row) {
                if let Ok(addon_id) = row.try_get::<String, _>("AddonId") {
                    usable_default_addons.push(addon_id);
                }
            }
            let (failure, warning) =
                installation_findings(&self.pool, &row, host_version.as_ref()).await;
            failures.extend(failure);
            warnings.extend(warning);
        }
        // A configured default that cannot run is silent otherwise: nothing
        // fails until an operator adds a source URL no add-on claims.
        if let Some(finding) =
            default_remote_stream_addon_finding(&self.pool, &usable_default_addons).await
        {
            warnings.push(finding);
        }

        if !failures.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                failures.into_iter().take(5).collect::<Vec<_>>().join("; "),
                "One or more add-ons have an invalid trust or package state",
            )
            .with_link("/settings/addons"));
        }
        if !warnings.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                warnings.into_iter().take(5).collect::<Vec<_>>().join("; "),
                "One or more add-ons need attention",
            )
            .with_link("/settings/addons"));
        }
        Ok(HealthCheckResult::pass(self.title()).with_link("/settings/addons"))
    }
}

async fn tool_maintenance_findings(pool: &Pool<Db>, row: &DbRow) -> Result<Vec<String>> {
    use ersatzrs_domain::{
        AddonToolMaintenanceOutcome as Outcome, AddonToolMaintenanceState as State,
    };
    let mut warnings = Vec::new();
    let Some(manifest) = row
        .try_get::<String, _>("ManifestJson")
        .ok()
        .and_then(|json| serde_json::from_str::<AddonManifest>(&json).ok())
        .filter(|manifest| manifest.validate().is_ok())
    else {
        return Ok(warnings);
    };
    let repository = crate::repositories::SqliteAddonMediaListRepository(pool.clone());
    for tool_key in &manifest.permissions.tool_update {
        let Some(attempt) = repository
            .tool_maintenance_history(&manifest.id, tool_key)
            .await
            .map_err(|error| ersatzrs_core::health::HealthCheckError::Database(error.to_string()))?
            .into_iter()
            .next()
        else {
            continue;
        };
        let code = match attempt.diagnostic_code.as_deref() {
            Some("tool-cleanup-required") => Some("tool-cleanup-required"),
            Some("tool-busy") => None,
            _ if attempt.outcome == Some(Outcome::ManualUpdateRequired) => {
                Some("unsupported_installation")
            }
            _ if attempt.state == State::Failed || attempt.outcome == Some(Outcome::Failed) => {
                Some(match attempt.diagnostic_code.as_deref() {
                    Some("version_probe_failed") => "version_probe_failed",
                    Some("tool_unavailable") => "tool_unavailable",
                    _ => "update_failed",
                })
            }
            _ => None,
        };
        if let Some(code) = code {
            warnings.push(format!(
                "Tool maintenance {}/{tool_key}: {code}",
                manifest.id
            ));
        }
    }
    Ok(warnings)
}

/// Whether one installation could serve as the default remote-stream add-on.
fn default_addon_is_usable(row: &DbRow) -> bool {
    let enabled = row
        .try_get::<i64, _>("Enabled")
        .is_ok_and(|value| value != 0);
    if !enabled {
        return false;
    }
    row.try_get::<String, _>("ManifestJson")
        .ok()
        .and_then(|json| serde_json::from_str::<AddonManifest>(&json).ok())
        .is_some_and(|manifest| manifest.has_remote_stream_play_capability())
}

/// Report a configured default remote-stream add-on that cannot be used.
async fn default_remote_stream_addon_finding(pool: &Pool<Db>, usable: &[String]) -> Option<String> {
    let configured =
        sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(crate::addons::DEFAULT_REMOTE_STREAM_ADDON_KEY)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()?;
    let configured = configured.trim();
    if configured.is_empty() || usable.iter().any(|addon_id| addon_id == configured) {
        return None;
    }
    Some(format!(
        "default remote stream processor '{configured}' is not installed, not enabled, or cannot play remote streams"
    ))
}

fn local_source_finding(row: &DbRow) -> Option<String> {
    let id = row
        .try_get::<String, _>("Id")
        .unwrap_or_else(|_| "local repository".to_owned());
    let kind = row
        .try_get::<String, _>("Kind")
        .unwrap_or_else(|_| "local".to_owned());
    row.try_get::<Option<String>, _>("LastError")
        .ok()
        .flatten()
        .map(|error| format!("{kind} source {id} last scan failed: {error}"))
}

#[allow(clippy::too_many_lines)]
async fn installation_findings(
    pool: &Pool<Db>,
    row: &DbRow,
    host_version: Option<&Version>,
) -> (Option<String>, Option<String>) {
    let id = row
        .try_get::<String, _>("AddonId")
        .unwrap_or_else(|_| "add-on".to_owned());
    let enabled = row
        .try_get::<i64, _>("Enabled")
        .is_ok_and(|value| value != 0);
    let manifest = row
        .try_get::<String, _>("ManifestJson")
        .ok()
        .and_then(|json| serde_json::from_str::<AddonManifest>(&json).ok());
    let manifest_valid = manifest.as_ref().is_some_and(|manifest| {
        manifest.validate().is_ok()
            && manifest.id == id
            && host_version.is_some_and(|version| manifest.host_version.matches(version))
    });
    let content_valid = match (
        row.try_get::<String, _>("InstallPath").ok(),
        row.try_get::<String, _>("ContentSha256").ok(),
    ) {
        (Some(path), Some(content_sha256)) => {
            crate::addons::verify_addon_content_path(std::path::Path::new(&path), &content_sha256)
                .await
                .is_ok()
        }
        _ => false,
    };
    let dependencies_valid = if enabled {
        if let Some(manifest) = manifest.as_ref().filter(|_| manifest_valid) {
            crate::addons::validate_installed_dependencies(pool, manifest)
                .await
                .is_ok()
        } else {
            false
        }
    } else {
        true
    };
    let storage_valid = if enabled
        && manifest
            .as_ref()
            .is_some_and(|manifest| manifest.media_list_storage.is_some())
    {
        sqlx::query_scalar::<_, String>(
            r#"SELECT "RootPath" FROM "AddonMediaSource" WHERE "AddonId" = ?"#,
        )
        .bind(&id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some_and(|path| std::path::Path::new(&path).is_dir())
    } else {
        true
    };
    if !storage_valid {
        return (
            Some(format!("{id} has unavailable managed media storage")),
            None,
        );
    }
    if !manifest_valid || !content_valid || !dependencies_valid {
        return (
            Some(format!(
                "{id} has an invalid installed package or dependency state"
            )),
            None,
        );
    }
    let last_check_code = row
        .try_get::<Option<String>, _>("LastCheckCode")
        .ok()
        .flatten();
    let no_readiness = last_check_code.is_none();
    let readiness_status = row
        .try_get::<Option<String>, _>("LastCheckStatus")
        .ok()
        .flatten()
        .and_then(|value| AddonCheckStatus::from_persisted(&value));
    let trust_tier = row
        .try_get::<String, _>("TrustTier")
        .unwrap_or_else(|_| "invalid".to_owned());
    // A readiness result that is not `ready` is reported ahead of the trust and
    // missing-result warnings: it describes an add-on that was enabled and has
    // since stopped being usable, which is the more actionable finding.
    let warning = if enabled
        && matches!(
            readiness_status,
            Some(AddonCheckStatus::Warning | AddonCheckStatus::Unavailable)
        ) {
        let status = readiness_status.unwrap_or(AddonCheckStatus::Unavailable);
        Some(format!(
            "{id} last reported {} readiness ({})",
            status.as_str(),
            last_check_code.as_deref().unwrap_or("no code")
        ))
    } else if enabled
        && row
            .try_get::<Option<String>, _>("SourceId")
            .ok()
            .flatten()
            .is_none()
        && row
            .try_get::<Option<String>, _>("RepositoryId")
            .ok()
            .flatten()
            .is_some()
    {
        Some(format!(
            "{id} is a legacy catalog installation with no managed update source"
        ))
    } else if enabled && trust_tier != "verified" {
        Some(format!("{id} is enabled with {trust_tier} trust"))
    } else if enabled && no_readiness {
        Some(format!("{id} is enabled without a readiness result"))
    } else {
        None
    };
    (None, warning)
}

#[cfg(test)]
mod tool_maintenance_tests {
    use super::*;

    #[tokio::test]
    async fn maintenance_health_uses_latest_attempt_and_projects_only_actionable_status() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            r#"CREATE TABLE "AddonInstallation" (
            "AddonId" TEXT PRIMARY KEY, "ManifestJson" TEXT NOT NULL)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let (_, table) = include_str!(
            "../../../../../migrations/202609041500_addon_list_lifecycle_foundation.sql"
        )
        .split_once("CREATE TABLE \"AddonToolMaintenanceJob\"")
        .unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE TABLE \"AddonToolMaintenanceJob\"{table}"
        )))
        .execute(&pool)
        .await
        .unwrap();
        let manifest = serde_json::json!({
            "manifest_version": 6, "id": "org.example.maintenance", "version": "1.0.0",
            "name": {"en-US": "Fixture"}, "summary": {"en-US": "Health fixture"},
            "license": "MIT", "source_url": "https://example.test/maintenance", "host_version": "*",
            "capabilities": [{"id": "addon.check.v1"}, {"id": "tool.update.v1"}],
            "entrypoints": {"any": {"path": "fixture", "kind": "native"}},
            "permissions": {"tool_update": ["TOOL_BIN"]},
            "settings": [{"key": "TOOL_BIN", "kind": "executable", "label": {"en-US": "Tool"}, "default": "fixture"}]
        });
        sqlx::query(r#"INSERT INTO "AddonInstallation" VALUES ('org.example.maintenance', ?)"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let row = sqlx::query(r#"SELECT * FROM "AddonInstallation""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "AddonToolMaintenanceJob"
            ("Id", "AddonId", "ToolKey", "Trigger", "State", "Outcome", "DiagnosticCode", "Diagnostic", "CreatedAt", "UpdatedAt")
            VALUES (1, 'org.example.maintenance', 'TOOL_BIN', 0, 4, 4, 'update_failed', 'SYNTHETIC_PRIVATE_DIAGNOSTIC', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z'),
                   (2, 'org.example.maintenance', 'TOOL_BIN', 0, 3, 0, NULL, NULL, '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')"#)
            .execute(&pool).await.unwrap();
        assert!(
            tool_maintenance_findings(&pool, &row)
                .await
                .unwrap()
                .is_empty(),
            "new success supersedes old failure"
        );
        for (state, outcome, code, expected) in [
            (3, Some(1), None, None),
            (
                3,
                Some(2),
                Some("unsupported_installation"),
                Some("unsupported_installation"),
            ),
            (
                4,
                Some(4),
                Some("version_probe_failed"),
                Some("version_probe_failed"),
            ),
            (
                4,
                Some(4),
                Some("tool_unavailable"),
                Some("tool_unavailable"),
            ),
            (
                4,
                Some(4),
                Some("SYNTHETIC_PRIVATE_DIAGNOSTIC"),
                Some("update_failed"),
            ),
            (2, None, Some("tool-busy"), None),
            (
                2,
                None,
                Some("tool-cleanup-required"),
                Some("tool-cleanup-required"),
            ),
        ] {
            sqlx::query(r#"UPDATE "AddonToolMaintenanceJob" SET "State" = ?, "Outcome" = ?, "DiagnosticCode" = ? WHERE "Id" = 2"#)
                .bind(state).bind(outcome).bind(code).execute(&pool).await.unwrap();
            let findings = tool_maintenance_findings(&pool, &row).await.unwrap();
            let expected: Vec<String> = expected
                .map(|code| format!("Tool maintenance org.example.maintenance/TOOL_BIN: {code}"))
                .into_iter()
                .collect();
            assert_eq!(findings, expected);
            assert!(!findings.join(" ").contains("SYNTHETIC_PRIVATE"));
        }
    }
}
