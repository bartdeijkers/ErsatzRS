use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use crate::db::DbRow;
use crate::remote_stream::{
    decode_persisted_liveness, resolve_remote_stream, RemoteStreamDefinition,
};
use ersatzrs_addon_contract::{validate_addon_id, AddonManifest, CAPABILITY_REMOTE_STREAM_PLAY_V1};
use ersatzrs_domain::RemoteStreamLiveness;
use semver::Version;
use std::path::Path;

pub(crate) struct RemoteStreamHealthCheck {
    pool: Pool<Db>,
}

pub(crate) struct RemoteStreamScheduleHealthCheck {
    pool: Pool<Db>,
}

pub(crate) struct RemoteStreamAddonReadinessHealthCheck {
    pool: Pool<Db>,
}

impl RemoteStreamHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

impl RemoteStreamScheduleHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

impl RemoteStreamAddonReadinessHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for RemoteStreamAddonReadinessHealthCheck {
    fn title(&self) -> &'static str {
        "Remote Stream add-on readiness"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = remote_stream_rows(&self.pool).await?;
        let host_version = Version::parse(env!("CARGO_PKG_VERSION")).ok();
        let unavailable = rows
            .iter()
            .filter(|row| {
                row.try_get::<Option<String>, _>("AddonId")
                    .ok()
                    .flatten()
                    .is_some()
                    && !remote_stream_addon_is_ready(row, host_version.as_ref())
            })
            .map(remote_stream_title)
            .collect::<Vec<_>>();

        if unavailable.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "{} Remote Streams use a missing, disabled, or incompatible add-on: {}",
                unavailable.len(),
                unavailable
                    .into_iter()
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "One or more Remote Stream add-ons are not ready",
        )
        .with_link("/addons"))
    }
}

#[async_trait]
impl HealthCheck for RemoteStreamScheduleHealthCheck {
    fn title(&self) -> &'static str {
        "Remote Stream schedule compatibility"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT p."Id" AS "PlayoutId", c."Name" AS "ChannelName", pbs."Message"
               FROM "PlayoutBuildStatus" pbs
               JOIN "Playout" p ON p."Id" = pbs."PlayoutId"
               JOIN "Channel" c ON c."Id" = p."ChannelId"
               WHERE pbs."Success" = 0
               ORDER BY c."Name", p."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        let incompatible = rows
            .into_iter()
            .filter_map(|row| {
                let message = row.try_get::<Option<String>, _>("Message").ok().flatten()?;
                let remote_stream_id = message
                    .strip_prefix("live Remote Stream ")?
                    .strip_suffix(" requires Classic Flood mode with a deterministic boundary")?;
                let playout_id = row.try_get::<i64, _>("PlayoutId").unwrap_or_default();
                let channel = row
                    .try_get::<Option<String>, _>("ChannelName")
                    .ok()
                    .flatten()
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| format!("Playout {playout_id}"));
                Some(format!("{channel} (Remote Stream {remote_stream_id})"))
            })
            .collect::<Vec<_>>();

        if incompatible.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "{} playouts contain a live Remote Stream outside Classic Flood: {}",
                incompatible.len(),
                incompatible
                    .into_iter()
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "One or more live Remote Streams use an incompatible schedule placement",
        )
        .with_link("/playouts"))
    }
}

#[async_trait]
impl HealthCheck for RemoteStreamHealthCheck {
    fn title(&self) -> &'static str {
        "Remote Streams"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = remote_stream_rows(&self.pool).await?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let mut invalid = Vec::new();
        let mut unknown = Vec::new();
        let mut warnings = Vec::new();
        // The joins above can return several rows for one Remote Stream, so a
        // finding is recorded once per stream rather than once per row.
        let mut reported = std::collections::BTreeSet::new();
        for row in rows {
            let id = row.try_get::<i64, _>("Id").unwrap_or_default();
            if !reported.insert(id) {
                continue;
            }
            let title = remote_stream_title(&row);
            let Some(definition) = remote_stream_definition(&row) else {
                invalid.push(title);
                continue;
            };
            if !remote_stream_source_is_valid(&row, &definition) {
                invalid.push(title);
                continue;
            }
            if definition.liveness == Some(RemoteStreamLiveness::Unknown) {
                unknown.push(title);
            } else if definition.liveness == Some(RemoteStreamLiveness::Finite)
                && !remote_stream_has_duration(&row, &definition)
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
        if !unknown.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "{} Remote Streams need playback behavior review: {}",
                    unknown.len(),
                    unknown.into_iter().take(5).collect::<Vec<_>>().join(", ")
                ),
                "One or more Remote Streams have unknown playback behavior",
            )
            .with_link("/media/remote/streams"));
        }
        Ok(HealthCheckResult::pass(self.title()))
    }
}

async fn remote_stream_rows(pool: &Pool<Db>) -> Result<Vec<DbRow>> {
    sqlx::query(
        r#"SELECT rs."Id", rs."Url", rs."AddonId", rs."Script", rs."Duration",
                  rs."FallbackQuery", rs."Liveness", rs."IsLive",
                  rsm."Title", mf."Path", mv."Duration" AS "ProbedDuration",
                  ai."Enabled" AS "AddonEnabled", ai."ManifestJson" AS "AddonManifestJson",
                  -- A replaced item plays from its local file, so that
                  -- file's duration is the one scheduling actually uses.
                  -- Read as a subquery so an assigned replacement cannot
                  -- multiply the rows this check walks.
                  (SELECT rmv."Duration"
                     FROM "AddonMediaListItem" ami
                     JOIN "AddonMediaListItemReplacement" amir
                       ON amir."AddonMediaListItemId" = ami."Id"
                     JOIN "MediaVersion" rmv ON rmv."Id" = amir."MediaVersionId"
                    WHERE ami."MediaItemId" = rs."Id"
                    ORDER BY rmv."Duration" DESC
                    LIMIT 1) AS "ReplacementDuration"
           FROM "RemoteStream" rs
           LEFT JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = rs."Id"
           LEFT JOIN "MediaVersion" mv ON mv."RemoteStreamId" = rs."Id"
           LEFT JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           LEFT JOIN "AddonInstallation" ai ON ai."AddonId" = rs."AddonId"
           ORDER BY rs."Id""#,
    )
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)
}

fn remote_stream_title(row: &DbRow) -> String {
    let id = row.try_get::<i64, _>("Id").unwrap_or_default();
    row.try_get::<Option<String>, _>("Title")
        .ok()
        .flatten()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| format!("Remote stream {id}"))
}

fn remote_stream_definition(row: &DbRow) -> Option<RemoteStreamDefinition> {
    let legacy_is_live = row.try_get::<i64, _>("IsLive").ok()? != 0;
    let liveness =
        decode_persisted_liveness(row.try_get::<i64, _>("Liveness").ok()?, legacy_is_live).ok()?;
    Some(RemoteStreamDefinition {
        url: row.try_get("Url").ok().flatten(),
        addon: row.try_get("AddonId").ok().flatten(),
        duration: row.try_get("Duration").ok().flatten(),
        fallback_query: row.try_get("FallbackQuery").ok().flatten(),
        liveness: Some(liveness),
        is_live: legacy_is_live,
        ..RemoteStreamDefinition::default()
    })
}

fn remote_stream_source_is_valid(row: &DbRow, definition: &RemoteStreamDefinition) -> bool {
    if row
        .try_get::<Option<String>, _>("Script")
        .ok()
        .flatten()
        .is_some()
    {
        return false;
    }
    let Some(addon_id) = definition.addon.as_deref() else {
        let mut source_definition = definition.clone();
        if source_definition.liveness == Some(RemoteStreamLiveness::Unknown) {
            source_definition.liveness = Some(RemoteStreamLiveness::Finite);
        }
        return resolve_remote_stream(&source_definition, Path::new("remote-stream.yml")).is_ok();
    };
    let has_url = definition
        .url
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty());
    has_url && validate_addon_id(addon_id).is_ok()
}

fn remote_stream_addon_is_ready(row: &DbRow, host_version: Option<&Version>) -> bool {
    let Some(addon_id) = row.try_get::<Option<String>, _>("AddonId").ok().flatten() else {
        return true;
    };
    validate_addon_id(&addon_id).is_ok()
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
    // An assigned Local replacement is what plays, so its probed duration
    // answers the question this check asks even when the provider supplied
    // nothing usable.
    let replacement_duration = row
        .try_get::<Option<String>, _>("ReplacementDuration")
        .ok()
        .flatten();
    definition
        .duration
        .as_deref()
        .into_iter()
        .chain(probed_duration.as_deref())
        .chain(replacement_duration.as_deref())
        .any(|duration| {
            let duration = duration.trim();
            !duration.is_empty() && duration != "00:00:00" && !duration.starts_with("00:00:00.")
        })
}

#[cfg(all(test, not(feature = "mysql")))]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn health_pool() -> Pool<Db> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        for statement in [
            r#"CREATE TABLE "RemoteStream" (
                "Id" INTEGER, "Url" TEXT, "AddonId" TEXT, "Script" TEXT,
                "Duration" TEXT, "FallbackQuery" TEXT, "Liveness" INTEGER, "IsLive" INTEGER
            )"#,
            r#"CREATE TABLE "RemoteStreamMetadata" ("RemoteStreamId" INTEGER, "Title" TEXT)"#,
            r#"CREATE TABLE "MediaVersion" (
                "Id" INTEGER, "RemoteStreamId" INTEGER, "Duration" TEXT
            )"#,
            r#"CREATE TABLE "MediaFile" ("MediaVersionId" INTEGER, "Path" TEXT)"#,
            r#"CREATE TABLE "AddonInstallation" (
                "AddonId" TEXT, "Enabled" INTEGER, "ManifestJson" TEXT
            )"#,
            r#"CREATE TABLE "AddonMediaListItem" ("Id" INTEGER, "MediaItemId" INTEGER)"#,
            r#"CREATE TABLE "AddonMediaListItemReplacement" (
                "AddonMediaListItemId" INTEGER, "MediaVersionId" INTEGER
            )"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("fixture statement");
        }
        pool
    }

    #[tokio::test]
    async fn liveness_and_duration_have_distinct_health_findings() {
        let pool = health_pool().await;
        sqlx::query(
            r#"INSERT INTO "RemoteStream"
                   ("Id", "Url", "AddonId", "Script", "Duration", "FallbackQuery", "Liveness", "IsLive")
               VALUES (1, 'https://example.test/one', NULL, NULL, NULL, NULL, 1, 0)"#,
        )
        .execute(&pool)
        .await
        .expect("finite stream");

        let check = RemoteStreamHealthCheck::new(pool.clone());
        let finite = check.check().await.expect("finite health result");
        assert_eq!(finite.status, HealthCheckStatus::Warning, "{finite:?}");
        assert_eq!(
            finite.brief_message,
            "One or more Remote Streams have no usable duration"
        );

        sqlx::query(r#"UPDATE "RemoteStream" SET "Liveness" = 0 WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .expect("unknown stream");
        let unknown = check.check().await.expect("unknown health result");
        assert_eq!(unknown.status, HealthCheckStatus::Warning, "{unknown:?}");
        assert_eq!(
            unknown.brief_message,
            "One or more Remote Streams have unknown playback behavior"
        );
    }

    #[tokio::test]
    async fn addon_readiness_is_separate_from_stream_structure() {
        let pool = health_pool().await;
        sqlx::query(
            r#"INSERT INTO "RemoteStream"
                   ("Id", "Url", "AddonId", "Script", "Duration", "FallbackQuery", "Liveness", "IsLive")
               VALUES (1, 'https://example.test/live', 'org.ersatzrs.addon.fixture',
                       NULL, NULL, NULL, 2, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("add-on stream");

        let structural = RemoteStreamHealthCheck::new(pool.clone())
            .check()
            .await
            .expect("structural health result");
        assert_eq!(structural.status, HealthCheckStatus::Pass, "{structural:?}");

        let readiness_check = RemoteStreamAddonReadinessHealthCheck::new(pool.clone());
        let unavailable = readiness_check
            .check()
            .await
            .expect("unavailable add-on result");
        assert_eq!(
            unavailable.status,
            HealthCheckStatus::Fail,
            "{unavailable:?}"
        );
        assert_eq!(
            unavailable.brief_message,
            "One or more Remote Stream add-ons are not ready"
        );

        let manifest = serde_json::json!({
            "manifest_version": 1,
            "id": "org.ersatzrs.addon.fixture",
            "version": "1.0.0",
            "name": {"en-US": "Fixture"},
            "summary": {"en-US": "Fixture Remote Stream add-on"},
            "license": "Zlib",
            "source_url": "https://example.test/source",
            "host_version": "*",
            "capabilities": [
                {"id": "addon.check.v1"},
                {"id": "remote-stream.list.v1"},
                {"id": "remote-stream.play.v1"}
            ],
            "entrypoints": {"any": {"path": "addon.sh", "kind": "posix_shell"}},
            "permissions": {},
            "settings": [],
            "panels": []
        });
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" ("AddonId", "Enabled", "ManifestJson")
               VALUES ('org.ersatzrs.addon.fixture', 1, ?)"#,
        )
        .bind(manifest.to_string())
        .execute(&pool)
        .await
        .expect("ready add-on");

        let ready = readiness_check.check().await.expect("ready add-on result");
        assert_eq!(ready.status, HealthCheckStatus::Pass, "{ready:?}");
    }

    /// An item whose provider copy is unusable still plays once a Local
    /// replacement is assigned, so the duration warning must stop naming it.
    #[tokio::test]
    async fn assigned_replacement_supplies_the_missing_duration() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        for statement in [
            r#"CREATE TABLE "RemoteStream" (
                "Id" INTEGER, "Url" TEXT, "AddonId" TEXT, "Script" TEXT,
                "Duration" TEXT, "FallbackQuery" TEXT, "Liveness" INTEGER, "IsLive" INTEGER
            )"#,
            r#"CREATE TABLE "RemoteStreamMetadata" ("RemoteStreamId" INTEGER, "Title" TEXT)"#,
            r#"CREATE TABLE "MediaVersion" (
                "Id" INTEGER, "RemoteStreamId" INTEGER, "Duration" TEXT
            )"#,
            r#"CREATE TABLE "MediaFile" ("MediaVersionId" INTEGER, "Path" TEXT)"#,
            r#"CREATE TABLE "AddonInstallation" (
                "AddonId" TEXT, "Enabled" INTEGER, "ManifestJson" TEXT
            )"#,
            r#"CREATE TABLE "AddonMediaListItem" ("Id" INTEGER, "MediaItemId" INTEGER)"#,
            r#"CREATE TABLE "AddonMediaListItemReplacement" (
                "AddonMediaListItemId" INTEGER, "MediaVersionId" INTEGER
            )"#,
            // A direct source keeps the definition valid without an add-on.
            r#"INSERT INTO "RemoteStream"
                   ("Id", "Url", "AddonId", "Script", "Duration", "FallbackQuery", "Liveness", "IsLive")
               VALUES (1, 'https://example.test/one', NULL, NULL, NULL, NULL, 1, 0)"#,
            r#"INSERT INTO "RemoteStreamMetadata" ("RemoteStreamId", "Title")
               VALUES (1, 'Replaced item')"#,
            // The provider copy probed as zero-length.
            r#"INSERT INTO "MediaVersion" ("Id", "RemoteStreamId", "Duration")
               VALUES (10, 1, '00:00:00')"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("fixture statement");
        }

        let check = RemoteStreamHealthCheck::new(pool.clone());
        let before = check.check().await.expect("check without replacement");
        assert_eq!(before.status, HealthCheckStatus::Warning, "{before:?}");
        assert!(before.message.contains("Replaced item"), "{before:?}");

        for statement in [
            r#"INSERT INTO "MediaVersion" ("Id", "RemoteStreamId", "Duration")
               VALUES (20, NULL, '00:27:39')"#,
            r#"INSERT INTO "AddonMediaListItem" ("Id", "MediaItemId") VALUES (5, 1)"#,
            r#"INSERT INTO "AddonMediaListItemReplacement"
                   ("AddonMediaListItemId", "MediaVersionId") VALUES (5, 20)"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("replacement statement");
        }

        let after = check.check().await.expect("check with replacement");
        assert_eq!(after.status, HealthCheckStatus::Pass, "{after:?}");
        assert!(!after.message.contains("Replaced item"), "{after:?}");
    }

    #[tokio::test]
    async fn incompatible_live_source_schedule_has_a_separate_health_finding() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        for statement in [
            r#"CREATE TABLE "Channel" ("Id" INTEGER, "Name" TEXT)"#,
            r#"CREATE TABLE "Playout" ("Id" INTEGER, "ChannelId" INTEGER)"#,
            r#"CREATE TABLE "PlayoutBuildStatus" (
                "PlayoutId" INTEGER, "Success" INTEGER, "Message" TEXT
            )"#,
            r#"INSERT INTO "Channel" ("Id", "Name") VALUES
                (1, 'Live channel'), (2, 'Other channel')"#,
            r#"INSERT INTO "Playout" ("Id", "ChannelId") VALUES (10, 1), (20, 2)"#,
            r#"INSERT INTO "PlayoutBuildStatus" ("PlayoutId", "Success", "Message") VALUES
                (10, 0, 'live Remote Stream 42 requires Classic Flood mode with a deterministic boundary'),
                (20, 0, 'unrelated playout failure')"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("fixture statement");
        }

        let result = RemoteStreamScheduleHealthCheck::new(pool)
            .check()
            .await
            .expect("schedule compatibility check");
        assert_eq!(result.status, HealthCheckStatus::Fail, "{result:?}");
        assert!(result.message.contains("Live channel"), "{result:?}");
        assert!(result.message.contains("Remote Stream 42"), "{result:?}");
        assert!(!result.message.contains("Other channel"), "{result:?}");
        assert_eq!(
            result.link.as_ref().map(|link| link.link.as_str()),
            Some("/playouts")
        );
    }
}
