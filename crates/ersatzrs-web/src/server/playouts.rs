//! Server functions for playout list and Sequential (YAML) playout editor.
//! Ports:
//! - `Pages/Playouts.razor` → [`list_playouts`]
//! - `Pages/ScriptedPlayoutEditor.razor` → [`get_sequential_playout`],
//!   [`save_sequential_playout`], [`erase_playout_items_and_history`],
//!   [`validate_yaml_playout`]
//!
//! Divergence: `ErsatzTV` stores a file *path* in `ScheduleFile`.
//! `ErsatzRS` stores the raw YAML *content* directly. The column name is kept
//! for DB compatibility; only the stored value changes.
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn schedule_kind_label(kind: i32) -> &'static str {
    match kind {
        1 => "Classic",
        2 => "Block",
        3 => "Sequential",
        4 => "Scripted",
        20 => "JSON (dizqueTV)",
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Shared value types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlayoutListRow {
    pub id: i64,
    pub channel_number: String,
    pub channel_name: String,
    pub schedule_kind: i32,
    pub schedule_kind_label: String,
    pub schedule_name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutTimelineItem {
    pub title: String,
    pub start_ms: i64,
    pub finish_ms: i64,
    pub filler_kind: i32,
    pub scheduling_context: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SequentialPlayoutForm {
    pub id: i64,
    /// "{Number} — {Name}"
    pub channel_label: String,
    /// Raw YAML content (empty string when NULL in DB).
    pub yaml_content: String,
}

// ---------------------------------------------------------------------------
// Server functions
// ---------------------------------------------------------------------------

#[server]
pub async fn list_playouts() -> Result<Vec<PlayoutListRow>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT p."Id",
                  COALESCE(c."Number", '') AS "ChannelNumber",
                  COALESCE(c."Name",   '') AS "ChannelName",
                  p."ScheduleKind",
                  COALESCE(ps."Name",  '') AS "ScheduleName"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           LEFT JOIN "ProgramSchedule" ps ON ps."Id" = p."ProgramScheduleId"
           ORDER BY c."Number", p."Id""#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let kind: i32 = r.try_get("ScheduleKind").unwrap_or(0);
            PlayoutListRow {
                id: r.try_get("Id").unwrap_or(0),
                channel_number: r.try_get("ChannelNumber").unwrap_or_default(),
                channel_name: r.try_get("ChannelName").unwrap_or_default(),
                schedule_kind: kind,
                schedule_kind_label: schedule_kind_label(kind).to_string(),
                schedule_name: r.try_get("ScheduleName").unwrap_or_default(),
            }
        })
        .collect())
}

#[server]
pub async fn list_playout_items(
    playout_id: i64,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<PlayoutTimelineItem>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT
                  COALESCE(
                      pi."CustomTitle",
                      (SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = pi."MediaItemId" LIMIT 1),
                      (SELECT "Title" FROM "EpisodeMetadata" WHERE "EpisodeId" = pi."MediaItemId" LIMIT 1),
                      (SELECT "Title" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = pi."MediaItemId" LIMIT 1),
                      ''
                  ) AS "Title",
                  pi."Start",
                  pi."Finish",
                  pi."FillerKind" AS "FillerKind",
                  pi."SchedulingContext" AS "SchedulingContext"
           FROM "PlayoutItem" pi
           WHERE pi."PlayoutId" = ?
             AND pi."Finish" > ?
             AND pi."Start" < ?
           ORDER BY pi."Start""#,
    )
    .bind(playout_id)
    .bind(format_epoch_ms(from_ms)?)
    .bind(format_epoch_ms(to_ms)?)
    .fetch_all(&state.pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            let filler_kind = i32::try_from(row.try_get::<i64, _>("FillerKind")?).unwrap_or(0);

            let start: String = row.try_get("Start")?;
            let finish: String = row.try_get("Finish")?;

            Ok(PlayoutTimelineItem {
                title: row.try_get("Title")?,
                start_ms: parse_timeline_datetime(&start)?,
                finish_ms: parse_timeline_datetime(&finish)?,
                filler_kind,
                scheduling_context: row.try_get("SchedulingContext").ok(),
            })
        })
        .collect()
}

fn format_epoch_ms(epoch_ms: i64) -> Result<String, ServerFnError> {
    chrono::DateTime::from_timestamp_millis(epoch_ms)
        .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
        .ok_or_else(|| ServerFnError::new("invalid timeline timestamp"))
}

fn parse_timeline_datetime(value: &str) -> Result<i64, ServerFnError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.timestamp_millis())
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .map(|parsed| parsed.and_utc().timestamp_millis())
        })
        .map_err(|source| ServerFnError::new(format!("invalid playout timestamp: {source}")))
}

/// Loads a Sequential (YAML) playout (`ScheduleKind = 3`) by id.
/// Returns `None` when the id does not exist or the kind is not Sequential —
/// the page will redirect back to `/playouts` in that case.
///
/// Note: ScheduleKind 3 = Sequential → `YamlPlayoutEngine`. ScheduleKind 4 =
/// Scripted → `ScriptedSchedulingEngine` (Lua). This editor targets YAML, so
/// it must filter on 3.
#[server]
pub async fn get_sequential_playout(
    id: i64,
) -> Result<Option<SequentialPlayoutForm>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let row = sqlx::query(
        r#"SELECT p."Id",
                  COALESCE(c."Number",         '') AS "ChannelNumber",
                  COALESCE(c."Name",           '') AS "ChannelName",
                  COALESCE(p."ScheduleFile",   '') AS "ScheduleFile"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE p."Id" = ? AND p."ScheduleKind" = 3"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(row.map(|r| {
        let number: String = r.try_get("ChannelNumber").unwrap_or_default();
        let name: String = r.try_get("ChannelName").unwrap_or_default();
        let channel_label = format!("{number} \u{2014} {name}");
        SequentialPlayoutForm {
            id: r.try_get("Id").unwrap_or(0),
            channel_label,
            yaml_content: r.try_get("ScheduleFile").unwrap_or_default(),
        }
    }))
}

/// Persists the YAML content for a Sequential playout and kicks off a
/// background playout rebuild so scheduled items stay current with the new
/// definition.
///
/// Mirrors `UpdateScriptedPlayoutHandler.cs`: saves `ScheduleFile` then
/// queues channel-refresh work.  ErsatzRS has no per-channel XMLTV file
/// cache (XMLTV is served on-the-fly), so the equivalent is a fire-and-forget
/// reset build via [`YamlPlayoutEngine`].
#[server]
pub async fn save_sequential_playout(id: i64, yaml_content: String) -> Result<(), ServerFnError> {
    use std::sync::Arc;

    use crate::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleFile" = ? WHERE "Id" = ? AND "ScheduleKind" = 3"#)
        .bind(&yaml_content)
        .bind(id)
        .execute(&state.pool)
        .await?;

    let repos = Arc::clone(&state.repositories);
    let yaml = yaml_content;
    tokio::task::spawn(async move {
        rebuild_yaml_playout(&repos, id, &yaml).await;
    });

    Ok(())
}

/// Runs a reset build for a Sequential (YAML) playout in the background via
/// the canonical [`PlayoutBuilder`] dispatcher.
///
/// Uses `yaml_reference_data` to populate existing items/history and all
/// media keys declared in the definition, then delegates to
/// `PlayoutBuilder::default().build()` which routes Sequential playouts to
/// `YamlPlayoutEngine`.  All errors are logged and swallowed — callers must
/// not rely on success.
async fn rebuild_yaml_playout(
    repos: &ersatzrs_infra::repositories::Repositories,
    id: i64,
    yaml_content: &str,
) {
    use chrono::{Duration, Utc};
    use ersatzrs_core::playout_builder::PlayoutBuilder;
    use ersatzrs_core::repositories::PlayoutRepository;
    use ersatzrs_core::scheduling::{PlayoutBuildMode, SchedulingRequest};

    let playout = match repos.playout.get_with_anchor(id).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(id, error = %e, "yaml rebuild: could not load playout");
            return;
        }
    };

    let reference_data = match repos
        .scheduling
        .yaml_reference_data(id, yaml_content, Duration::days(30))
        .await
    {
        Ok(rd) => rd,
        Err(e) => {
            tracing::warn!(id, error = %e, "yaml rebuild: could not build reference data");
            return;
        }
    };

    let now = Utc::now();
    let request = SchedulingRequest {
        playout: playout.clone(),
        start: now,
        finish: now + Duration::days(2),
        mode: PlayoutBuildMode::Reset,
        reference_data,
    };

    let result = match PlayoutBuilder::default().build(request) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(id, error = %e, "yaml rebuild: engine error");
            return;
        }
    };

    let mut playout_with_anchor = playout;
    playout_with_anchor.anchor = result.anchor.clone();
    if let Err(e) = repos
        .playout
        .apply_build_result(&result, &playout_with_anchor)
        .await
    {
        tracing::warn!(id, error = %e, "yaml rebuild: failed to persist result");
    }
}

/// Resets a playout to a blank state in a single transaction — equivalent to
/// ErsatzTV's `ErasePlayoutHistory` command.
///
/// Mirrors `ErasePlayoutHistoryHandler.cs`: randomises `Seed`, clears
/// `Anchor` and `OnDemandCheckpoint`, then deletes every build-time row
/// (`PlayoutItem`, `PlayoutHistory`, `PlayoutProgramScheduleAnchor`,
/// `RerunHistory`, `PlayoutGap`, `PlayoutBuildStatus`).
#[server]
pub async fn erase_playout_items_and_history(id: i64) -> Result<(), ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let mut tx = state.pool.begin().await?;

    // Randomise Seed (SQLite random() returns a 64-bit integer) and clear
    // Anchor + OnDemandCheckpoint on the Playout row.
    sqlx::query(
        r#"UPDATE "Playout"
           SET "Seed" = (SELECT random()), "OnDemandCheckpoint" = NULL
           WHERE "Id" = ?"#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // Remove the owned anchor row (nullable FK on Playout).
    sqlx::query(r#"DELETE FROM "PlayoutAnchor" WHERE "PlayoutId" = ?"#)
        .bind(id)
        .execute(&mut *tx)
        .await?;

    // Delete all build-time rows.
    for table in &[
        "PlayoutItem",
        "PlayoutHistory",
        "PlayoutProgramScheduleAnchor",
        "RerunHistory",
        "PlayoutGap",
        "PlayoutBuildStatus",
    ] {
        sqlx::query(&format!(r#"DELETE FROM "{table}" WHERE "PlayoutId" = ?"#))
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Validates YAML playout content server-side using [`YamlPlayoutEngine`].
/// Returns `Ok(())` on valid YAML, `Err` with the parse error message otherwise.
/// This function does not require `AppState`.
#[allow(clippy::unused_async)]
#[server]
pub async fn validate_yaml_playout(content: String) -> Result<(), ServerFnError> {
    use ersatzrs_core::scheduling::YamlPlayoutEngine;
    match YamlPlayoutEngine::definition_from_str(&content) {
        Ok(_) => Ok(()),
        Err(e) => Err(ServerFnError::new(e.to_string())),
    }
}
