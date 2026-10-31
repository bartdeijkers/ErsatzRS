//! Bounded persisted maintenance diagnostics, never output or client acceptance.

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use ersatzrs_core::{
    health::{HealthCheck, HealthCheckResult, HealthCheckStatus, Result},
    repositories::{
        teletext_guide_refresh_stale_ceiling, teletext_guide_refresh_status,
        TeletextGuideRefreshStatus, MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES,
    },
};
use sqlx::Row as _;

use crate::{
    db::{DbPool, DbRow},
    repositories::common::{decode_datetime, decode_string_column, encode_datetime},
};

/// Inspect registered guide-maintenance state; it does not certify invocation,
/// native compilation, stream delivery, client display or runtime adoption.
/// Upgrade authorization coverage is currently limited to applicable Base5;
/// historical Effective6 needs exact live-scope qualification before counting.
pub struct TeletextGuideRefreshHealthCheck {
    pool: DbPool,
}

#[derive(Default)]
struct Counts {
    fresh: usize,
    stale: usize,
    unavailable: usize,
    revoked: usize,
    review: usize,
    refreshing: usize,
    invalid: usize,
}

impl TeletextGuideRefreshHealthCheck {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn check_at(&self, now: DateTime<Utc>) -> Result<HealthCheckResult> {
        // Read only the declared version, not arbitrary configuration/private text.
        let enabled: Option<i64> = sqlx::query_scalar(
            r#"SELECT CASE WHEN LENGTH(CAST("Value" AS BLOB))<=65536
            AND json_valid(CAST("Value" AS TEXT)) THEN CASE
                WHEN json_type(CAST("Value" AS TEXT),'$.version')='integer'
                THEN json_extract(CAST("Value" AS TEXT),'$.version') END END
            FROM "ConfigElement" WHERE "Key"='teletext.output.policy'"#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(super::super::sqlx_error)?
        .flatten();
        if enabled != Some(4) {
            return Ok(self.result(HealthCheckStatus::Info,
                "Automatic guide maintenance requires runtime policy version 4; this check does not activate it."));
        }
        let missing = self.missing_base_authorizations(now).await?;
        // No Document or Source BLOB is returned. SQLite extracts only a bounded
        // UTC capture scalar from the already stored private source association.
        let rows = sqlx::query(r#"SELECT c."LastSuccessAt",c."FirstFailureAt",c."Failure",c."Revoked",c."LeaseUntil",
            CASE WHEN c."LeaseOwner" IS NOT NULL THEN 1 ELSE 0 END AS "Refreshing",
            CASE WHEN s."Id" IS NOT NULL AND s."ChannelId"=c."ChannelId" AND s."Role"=c."Role"
                AND (c."RuntimeId" IS NULL OR r."Id" IS NOT NULL) THEN 1 ELSE 0 END AS "Association",
            COALESCE(r."ValidUntil",s."ValidUntil") AS "ValidUntil",
            CASE WHEN LENGTH(COALESCE(r."Source",s."Source"))<=?
                AND json_valid(CAST(COALESCE(r."Source",s."Source") AS TEXT))
                THEN json_extract(CAST(COALESCE(r."Source",s."Source") AS TEXT),'$.captured_at') END AS "CapturedAt"
            FROM "TeletextGuideRefreshCurrent" c LEFT JOIN "TeletextGuideRefreshSeed" s ON s."Id"=c."SeedId"
            LEFT JOIN "TeletextGuideRuntimeSnapshot" r ON r."Id"=c."RuntimeId" AND r."SeedId"=c."SeedId"
            ORDER BY c."SeedId" LIMIT 257"#)
            .bind(i64::try_from(MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES).unwrap_or(i64::MAX))
            .fetch_all(&self.pool).await.map_err(super::super::sqlx_error)?;
        if rows.is_empty() && missing == 0 {
            return Ok(self.result(
                HealthCheckStatus::Info,
                "No registered guide maintenance origins.",
            ));
        }
        let incomplete = rows.len() > 256;
        let mut counts = Counts::default();
        for row in rows.iter().take(256) {
            if classify(row, now, &mut counts).is_err() {
                counts.invalid += 1;
            }
        }
        let status = if incomplete
            || missing > 0
            || counts.stale + counts.unavailable + counts.review + counts.invalid > 0
        {
            HealthCheckStatus::Warning
        } else {
            HealthCheckStatus::Pass
        };
        let mut message = format!("Stored guide maintenance: fresh={}, stale={}, unavailable={}, revoked={}; jobs: needs-review={}, owned={}; invalid metadata={}, incomplete={}. This is not stream delivery or client-display evidence.",
            counts.fresh,counts.stale,counts.unavailable,counts.revoked,counts.review,counts.refreshing,counts.invalid,incomplete);
        if missing > 0 {
            let _ = write!(message, " Missing Base guide refresh authorization: {missing}. Reprepare the Base guide, request publication and publish explicitly; ordinary audio/video playback remains available.");
        }
        Ok(self.result(status, &message))
    }

    async fn missing_base_authorizations(&self, now: DateTime<Utc>) -> Result<i64> {
        // Match the ordinary Base-vs-Scripted selector's exact persisted clock.
        // This is a scalar-only upgrade check, never an implicit seed backfill.
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextAcceptedGenerationCurrent" selected
            JOIN "TeletextAcceptedGeneration" g ON g."Id"=selected."GenerationId" AND g."ChannelId"=selected."ChannelId"
            JOIN "Channel" channel ON channel."Id"=selected."ChannelId"
            WHERE g."ArtifactVersion"=5
            AND EXISTS (SELECT 1 FROM "ChannelTeletextBinding" b WHERE b."ChannelId"=selected."ChannelId" AND b."Enabled"=1)
            AND NOT EXISTS (SELECT 1 FROM "PlayoutTeletextInterval" i JOIN "Playout" p ON p."Id"=i."PlayoutId"
                WHERE p."ChannelId"=selected."ChannelId" AND i."Start"<=? AND i."Finish">?)
            AND NOT EXISTS (SELECT 1 FROM "TeletextGuideRefreshCurrent" c JOIN "TeletextGuideRefreshSeed" s
                ON s."Id"=c."SeedId" AND s."ChannelId"=c."ChannelId" AND s."Role"=c."Role"
                WHERE c."ChannelId"=selected."ChannelId" AND c."Role"=1 AND s."GenerationId"=g."Id")"#)
            .bind(encode_datetime(now)).bind(encode_datetime(now))
            .fetch_one(&self.pool).await.map_err(super::super::sqlx_error)
    }

    fn result(&self, status: HealthCheckStatus, message: &str) -> HealthCheckResult {
        HealthCheckResult::new(self.title(), status, message, message)
    }
}

fn date(row: &DbRow, column: &'static str) -> std::result::Result<DateTime<Utc>, ()> {
    let value = decode_string_column(row, column).map_err(|_| ())?;
    decode_datetime("guide maintenance clock", &value).map_err(|_| ())
}

fn optional_date(
    row: &DbRow,
    column: &'static str,
) -> std::result::Result<Option<DateTime<Utc>>, ()> {
    row.try_get::<Option<String>, _>(column)
        .map_err(|_| ())?
        .map(|value| decode_datetime("guide maintenance clock", &value).map_err(|_| ()))
        .transpose()
}

fn classify(row: &DbRow, now: DateTime<Utc>, counts: &mut Counts) -> std::result::Result<(), ()> {
    let captured = date(row, "CapturedAt")?;
    let last_success = date(row, "LastSuccessAt")?;
    let until = date(row, "ValidUntil")?;
    let first_loss = optional_date(row, "FirstFailureAt")?;
    let lease_until = optional_date(row, "LeaseUntil")?;
    let revoked: i64 = row.try_get("Revoked").map_err(|_| ())?;
    let refreshing: i64 = row.try_get("Refreshing").map_err(|_| ())?;
    let association: i64 = row.try_get("Association").map_err(|_| ())?;
    let failure: Option<i64> = row.try_get("Failure").map_err(|_| ())?;
    if association != 1
        || !matches!(revoked, 0 | 1)
        || !matches!(refreshing, 0 | 1)
        || failure.is_some_and(|code| !(1..=4).contains(&code))
        || until <= captured
        || last_success < captured
        || first_loss.is_some_and(|loss| loss < captured)
        || (refreshing == 1) != lease_until.is_some()
    {
        return Err(());
    }
    let cap = teletext_guide_refresh_stale_ceiling(until, first_loss);
    match teletext_guide_refresh_status(
        now,
        captured,
        cap,
        revoked == 1,
        refreshing == 1,
        first_loss,
    ) {
        TeletextGuideRefreshStatus::Fresh => counts.fresh += 1,
        TeletextGuideRefreshStatus::Stale => counts.stale += 1,
        TeletextGuideRefreshStatus::Revoked => counts.revoked += 1,
        TeletextGuideRefreshStatus::Unavailable | TeletextGuideRefreshStatus::NeedsReview => {
            counts.unavailable += 1;
        }
    }
    counts.review += usize::from(failure == Some(3));
    counts.refreshing += usize::from(refreshing == 1);
    Ok(())
}

#[async_trait::async_trait]
impl HealthCheck for TeletextGuideRefreshHealthCheck {
    fn title(&self) -> &'static str {
        "Teletext guide maintenance"
    }
    async fn check(&self) -> Result<HealthCheckResult> {
        self.check_at(Utc::now()).await
    }
}
