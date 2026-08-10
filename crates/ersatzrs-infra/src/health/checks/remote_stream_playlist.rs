use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use crate::remote_stream::parse_script_command;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const CONFIG_KEY_REMOTE_STREAM_EXTRACTORS: &str = "settings.remote_stream_extractors";

pub(crate) struct RemoteStreamPlaylistHealthCheck {
    pool: Pool<Db>,
}

impl RemoteStreamPlaylistHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[derive(Deserialize)]
struct HealthManifest {
    name: Option<String>,
    extractor: String,
    folder: Option<String>,
    #[serde(default)]
    sync: HealthSync,
}

#[derive(Deserialize)]
#[serde(default)]
struct HealthSync {
    interval: String,
}

impl Default for HealthSync {
    fn default() -> Self {
        Self {
            interval: "24h".to_owned(),
        }
    }
}

#[derive(Deserialize)]
struct HealthExtractor {
    name: String,
    windows: String,
    unix: String,
}

#[derive(Deserialize)]
struct HealthSidecar {
    version: u32,
    last_sync: DateTime<Utc>,
    last_result: HealthOutcome,
    #[serde(default)]
    entries: Vec<HealthEntry>,
}

#[derive(Deserialize)]
struct HealthOutcome {
    kind: String,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct HealthEntry {
    duration_seconds: Option<u64>,
    active: bool,
}

#[async_trait]
impl HealthCheck for RemoteStreamPlaylistHealthCheck {
    fn title(&self) -> &'static str {
        "Remote Stream playlists"
    }

    #[allow(clippy::too_many_lines)]
    async fn check(&self) -> Result<HealthCheckResult> {
        let roots = remote_stream_library_roots(&self.pool).await?;
        let manifests = roots
            .iter()
            .flat_map(|root| collect_manifests(root))
            .collect::<Vec<_>>();
        if manifests.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }
        let extractors = load_extractors(&self.pool).await?;
        let mut failures = Vec::new();
        let mut warnings = Vec::new();
        let mut truncated = Vec::new();
        for path in manifests {
            let fallback_name = manifest_name(&path);
            let Ok(yaml) = std::fs::read_to_string(&path) else {
                failures.push(format!("{fallback_name} cannot be read"));
                continue;
            };
            let Ok(manifest) = serde_yaml::from_str::<HealthManifest>(&yaml) else {
                failures.push(format!("{fallback_name} is invalid"));
                continue;
            };
            let name = manifest
                .name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&fallback_name)
                .to_owned();
            let Some(extractor) = extractors.get(&manifest.extractor.to_ascii_lowercase()) else {
                failures.push(format!("{name} has an unknown extractor"));
                continue;
            };
            if !extractor_available(extractor) {
                failures.push(format!("{name} has an unavailable extractor"));
                continue;
            }
            let interval =
                parse_interval(&manifest.sync.interval).unwrap_or(Duration::from_secs(0));
            if interval.is_zero() {
                failures.push(format!("{name} has an invalid sync interval"));
                continue;
            }
            let folder = manifest
                .folder
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&fallback_name);
            let sidecar_path = path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(folder)
                .join(".ersatzrs-playlist.json");
            let state: HealthSidecar = match std::fs::read_to_string(sidecar_path)
                .ok()
                .and_then(|value| serde_json::from_str::<HealthSidecar>(&value).ok())
            {
                Some(state) if state.version == 1 => state,
                _ => {
                    warnings.push(format!("{name} has never synchronized successfully"));
                    continue;
                }
            };
            if state.last_result.kind != "ok" {
                failures.push(format!("{name} last synchronization failed"));
                continue;
            }
            let stale_after = interval.checked_mul(2).unwrap_or(Duration::MAX);
            if Utc::now()
                .signed_duration_since(state.last_sync)
                .to_std()
                .is_ok_and(|age| age > stale_after)
            {
                warnings.push(format!("{name} is stale"));
            }
            let missing = state
                .entries
                .iter()
                .filter(|entry| {
                    entry.active && entry.duration_seconds.is_none_or(|value| value == 0)
                })
                .count();
            if missing > 0 {
                warnings.push(format!("{name} has {missing} entries without duration"));
            }
            if state.last_result.truncated {
                truncated.push(name);
            }
        }

        if !failures.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                failures.into_iter().take(5).collect::<Vec<_>>().join("; "),
                "One or more Remote Stream playlists cannot synchronize",
            )
            .with_link("/media/remote/streams"));
        }
        if !warnings.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                warnings.into_iter().take(5).collect::<Vec<_>>().join("; "),
                "One or more Remote Stream playlists need attention",
            )
            .with_link("/media/remote/streams"));
        }
        if !truncated.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Info,
                format!(
                    "The configured item limit truncates: {}",
                    truncated.into_iter().take(5).collect::<Vec<_>>().join(", ")
                ),
                "One or more Remote Stream playlists are intentionally truncated",
            )
            .with_link("/media/remote/streams"));
        }
        Ok(HealthCheckResult::pass(self.title()))
    }
}

async fn remote_stream_library_roots(pool: &Pool<Db>) -> Result<Vec<PathBuf>> {
    let rows = sqlx::query(
        r#"SELECT "lp"."Path", "l"."MediaKind"
           FROM "LibraryPath" AS "lp"
           JOIN "Library" AS "l" ON "l"."Id" = "lp"."LibraryId"
           JOIN "LocalLibrary" AS "ll" ON "ll"."Id" = "l"."Id"
           ORDER BY "lp"."Id""#,
    )
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?;
    Ok(rows
        .into_iter()
        .filter(|row| {
            row.try_get::<i64, _>("MediaKind")
                .is_ok_and(|value| value == 7)
                || row
                    .try_get::<String, _>("MediaKind")
                    .is_ok_and(|value| value == "RemoteStreams")
        })
        .filter_map(|row| row.try_get::<Option<String>, _>("Path").ok().flatten())
        .map(PathBuf::from)
        .collect())
}

async fn load_extractors(pool: &Pool<Db>) -> Result<BTreeMap<String, HealthExtractor>> {
    let raw: Option<String> =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(CONFIG_KEY_REMOTE_STREAM_EXTRACTORS)
            .fetch_optional(pool)
            .await
            .map_err(sqlx_error)?;
    let values: Vec<HealthExtractor> = raw
        .as_deref()
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or_default();
    Ok(values
        .into_iter()
        .map(|value| (value.name.trim().to_ascii_lowercase(), value))
        .collect())
}

fn extractor_available(extractor: &HealthExtractor) -> bool {
    let value = if cfg!(windows) {
        &extractor.windows
    } else {
        &extractor.unix
    };
    parse_script_command(value, Path::new("."))
        .ok()
        .is_some_and(|command| {
            let path = Path::new(&command.program);
            path.is_absolute() && path.is_file()
        })
}

fn collect_manifests(root: &Path) -> Vec<PathBuf> {
    let mut manifests = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_none_or(|value| !value.starts_with('.'))
                {
                    pending.push(path);
                }
            } else if is_manifest(&path) {
                manifests.push(path);
            }
        }
    }
    manifests.sort();
    manifests
}

fn is_manifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            let lower = value.to_ascii_lowercase();
            lower.ends_with(".playlist.yml") || lower.ends_with(".playlist.yaml")
        })
}

fn manifest_name(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("playlist");
    name.strip_suffix(".playlist.yml")
        .or_else(|| name.strip_suffix(".playlist.yaml"))
        .unwrap_or(name)
        .to_owned()
}

fn parse_interval(value: &str) -> Option<Duration> {
    let value = value.trim();
    let split = value.find(|character: char| !character.is_ascii_digit())?;
    let amount = value[..split].parse::<u64>().ok()?;
    let multiplier = match &value[split..].to_ascii_lowercase()[..] {
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return None,
    };
    amount.checked_mul(multiplier).map(Duration::from_secs)
}

#[cfg(all(test, not(feature = "mysql"), unix))]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn health_reports_missing_duration_and_unknown_extractor() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let root = temp.path().join("remote-streams");
        let folder = root.join("Fixture");
        std::fs::create_dir_all(&folder).expect("managed folder");
        std::fs::write(
            root.join("Fixture.playlist.yml"),
            "url: https://example.test/list\nextractor: fixture\n",
        )
        .expect("manifest");
        let state = serde_json::json!({
            "version": 1,
            "last_sync": Utc::now(),
            "last_result": {"kind": "ok", "truncated": false},
            "entries": [{"duration_seconds": null, "active": true}]
        });
        std::fs::write(
            folder.join(".ersatzrs-playlist.json"),
            serde_json::to_vec(&state).expect("state JSON"),
        )
        .expect("sidecar");

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        for statement in [
            r#"CREATE TABLE "Library" ("Id" INTEGER, "MediaKind" INTEGER)"#,
            r#"CREATE TABLE "LocalLibrary" ("Id" INTEGER)"#,
            r#"CREATE TABLE "LibraryPath" ("Id" INTEGER, "LibraryId" INTEGER, "Path" TEXT)"#,
            r#"CREATE TABLE "ConfigElement" ("Key" TEXT, "Value" TEXT)"#,
            r#"INSERT INTO "Library" ("Id", "MediaKind") VALUES (1, 7)"#,
            r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("fixture statement");
        }
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#)
            .bind(root.display().to_string())
            .execute(&pool)
            .await
            .expect("library path");
        let settings = serde_json::json!([{
            "name": "fixture",
            "windows": r"C:\fixture.bat",
            "unix": "/bin/sh"
        }]);
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(CONFIG_KEY_REMOTE_STREAM_EXTRACTORS)
            .bind(settings.to_string())
            .execute(&pool)
            .await
            .expect("settings");

        let check = RemoteStreamPlaylistHealthCheck::new(pool.clone());
        let result = check.check().await.expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result.message.contains("1 entries without duration"));

        sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = '[]' WHERE "Key" = ?"#)
            .bind(CONFIG_KEY_REMOTE_STREAM_EXTRACTORS)
            .execute(&pool)
            .await
            .expect("unknown extractor");
        let result = check.check().await.expect("failed health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains("unknown extractor"));
        assert!(!result.message.contains(&root.display().to_string()));
    }
}
