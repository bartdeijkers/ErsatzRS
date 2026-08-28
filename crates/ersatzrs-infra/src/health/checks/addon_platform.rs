use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use crate::db::DbRow;
use ersatzrs_addon_contract::{AddonCheckStatus, AddonManifest};
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
        .is_some_and(|manifest| {
            manifest.has_capability(ersatzrs_addon_contract::CAPABILITY_REMOTE_STREAM_PLAY_V1)
        })
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
