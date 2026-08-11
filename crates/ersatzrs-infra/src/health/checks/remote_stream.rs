use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use crate::db::DbRow;
use crate::remote_stream::{resolve_remote_stream, RemoteStreamDefinition};
use ersatzrs_addon_contract::{validate_addon_id, AddonManifest, CAPABILITY_REMOTE_STREAM_PLAY_V1};
use semver::Version;
use std::path::Path;

pub(crate) struct RemoteStreamHealthCheck {
    pool: Pool<Db>,
}

impl RemoteStreamHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for RemoteStreamHealthCheck {
    fn title(&self) -> &'static str {
        "Remote Streams"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT rs."Id", rs."Url", rs."AddonId", rs."Script", rs."Duration",
                      rs."FallbackQuery", rs."IsLive",
                      rsm."Title", mf."Path", mv."Duration" AS "ProbedDuration",
                      ai."Enabled" AS "AddonEnabled", ai."ManifestJson" AS "AddonManifestJson"
               FROM "RemoteStream" rs
               LEFT JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = rs."Id"
               LEFT JOIN "MediaVersion" mv ON mv."RemoteStreamId" = rs."Id"
               LEFT JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
               LEFT JOIN "AddonInstallation" ai ON ai."AddonId" = rs."AddonId"
               ORDER BY rs."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let mut invalid = Vec::new();
        let mut warnings = Vec::new();
        let host_version = Version::parse(env!("CARGO_PKG_VERSION")).ok();
        for row in rows {
            let title = remote_stream_title(&row);
            let definition = remote_stream_definition(&row);
            if !remote_stream_source_is_valid(&row, &definition, host_version.as_ref()) {
                invalid.push(title);
                continue;
            }
            if !remote_stream_is_live(&definition) && !remote_stream_has_duration(&row, &definition)
            {
                warnings.push(title);
            }
        }

        if !invalid.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                format!(
                    "{} Remote Stream definitions are invalid: {}",
                    invalid.len(),
                    invalid.into_iter().take(5).collect::<Vec<_>>().join(", ")
                ),
                "One or more Remote Stream definitions are invalid",
            )
            .with_link("/media/remote/streams"));
        }
        if !warnings.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "{} on-demand Remote Streams have no usable duration: {}",
                    warnings.len(),
                    warnings.into_iter().take(5).collect::<Vec<_>>().join(", ")
                ),
                "One or more Remote Streams have no usable duration",
            )
            .with_link("/media/remote/streams"));
        }
        Ok(HealthCheckResult::pass(self.title()))
    }
}

fn remote_stream_title(row: &DbRow) -> String {
    let id = row.try_get::<i64, _>("Id").unwrap_or_default();
    row.try_get::<Option<String>, _>("Title")
        .ok()
        .flatten()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| format!("Remote stream {id}"))
}

fn remote_stream_definition(row: &DbRow) -> RemoteStreamDefinition {
    RemoteStreamDefinition {
        url: row.try_get("Url").ok().flatten(),
        addon: row.try_get("AddonId").ok().flatten(),
        duration: row.try_get("Duration").ok().flatten(),
        fallback_query: row.try_get("FallbackQuery").ok().flatten(),
        is_live: row.try_get::<i64, _>("IsLive").unwrap_or_default() != 0,
        ..RemoteStreamDefinition::default()
    }
}

fn remote_stream_source_is_valid(
    row: &DbRow,
    definition: &RemoteStreamDefinition,
    host_version: Option<&Version>,
) -> bool {
    if row
        .try_get::<Option<String>, _>("Script")
        .ok()
        .flatten()
        .is_some()
    {
        return false;
    }
    let Some(addon_id) = definition.addon.as_deref() else {
        return resolve_remote_stream(definition, Path::new("remote-stream.yml")).is_ok();
    };
    let has_url = definition
        .url
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty());
    has_url
        && validate_addon_id(addon_id).is_ok()
        && row
            .try_get::<Option<i64>, _>("AddonEnabled")
            .ok()
            .flatten()
            .is_some_and(|enabled| enabled != 0)
        && row
            .try_get::<Option<String>, _>("AddonManifestJson")
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<AddonManifest>(&json).ok())
            .is_some_and(|manifest| {
                manifest.validate().is_ok()
                    && manifest.id == addon_id
                    && manifest.has_capability(CAPABILITY_REMOTE_STREAM_PLAY_V1)
                    && host_version.is_some_and(|version| manifest.host_version.matches(version))
            })
}

fn remote_stream_has_duration(row: &DbRow, definition: &RemoteStreamDefinition) -> bool {
    let probed_duration = row
        .try_get::<Option<String>, _>("ProbedDuration")
        .ok()
        .flatten();
    definition
        .duration
        .as_deref()
        .or(probed_duration.as_deref())
        .is_some_and(|duration| {
            let duration = duration.trim();
            !duration.is_empty() && duration != "00:00:00" && !duration.starts_with("00:00:00.")
        })
}

fn remote_stream_is_live(definition: &RemoteStreamDefinition) -> bool {
    definition.is_live
        || definition
            .url
            .as_deref()
            .is_some_and(|url| url.to_ascii_lowercase().starts_with("rtsp:"))
}
