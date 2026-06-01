//! Server functions for the playouts list/detail page and playout editors.
//!
//! Ports:
//! - `Pages/Playouts.razor` → [`list_playouts`], [`count_playouts`],
//!   [`list_playout_detail_items`], [`count_playout_detail_items`],
//!   [`delete_playout`], [`reset_playout`], [`reset_all_playouts`],
//!   [`save_playout_daily_reset`], [`update_external_json_playout`],
//!   [`get_show_filler_pref`], [`set_show_filler_pref`],
//!   [`list_locked_playouts`].
//! - `Application/Playouts/Queries/GetPagedPlayoutsHandler.cs`,
//!   `GetFuturePlayoutItemsByIdHandler.cs`,
//!   `Application/Playouts/Commands/{DeletePlayout,ResetAllPlayouts,UpdatePlayout,
//!   UpdateExternalJsonPlayout}Handler.cs`.
//! - `Application/ProgramSchedules/Queries/ProcessSchedulingContextHandler.cs`
//!   → [`process_scheduling_context`].
//! - `Pages/SequentialPlayoutEditor.razor` → [`get_sequential_playout`],
//!   [`save_sequential_playout`], [`erase_playout_items_and_history`],
//!   [`validate_yaml_playout`].
//! - `Pages/{Classic,Block,Scripted}PlayoutEditor.razor` →
//!   [`get_playout_kind_editor`], [`save_scripted_playout`],
//!   [`save_playout_default_deco`], [`erase_playout_items`].
//!
//! Divergence: `ErsatzTV` stores a file *path* in `ScheduleFile`. `ErsatzRS`
//! stores the raw YAML *content* directly for Sequential playouts. The column
//! name is kept for DB compatibility; only the stored value changes.
//!
//! Divergence: reset/refresh enqueue a `PlayoutBuildRequest` over the
//! in-process scheduler channel (the scheduler is the sole builder, sharing one
//! `EntityLocker`), mirroring ErsatzTV's `WorkerService` model. ErsatzRS serves
//! `channels.m3u`/XMLTV on-the-fly, so delete/external-json updates have no
//! per-channel guide-cache file to refresh — the DB change is the side effect.
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// `ScheduleKind` integer values (mirrors `PlayoutScheduleKind`).
pub const KIND_CLASSIC: i32 = 1;
pub const KIND_BLOCK: i32 = 2;
pub const KIND_SEQUENTIAL: i32 = 3;
pub const KIND_SCRIPTED: i32 = 4;
pub const KIND_EXTERNAL_JSON: i32 = 20;

// ---------------------------------------------------------------------------
// Shared value types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutListRow {
    pub id: i64,
    pub channel_number: String,
    pub channel_name: String,
    pub schedule_kind: i32,
    pub schedule_name: String,
    /// Raw YAML content (Sequential) or external-json path (`ExternalJson`).
    pub schedule_file: String,
    /// Current `DailyRebuildTime` as minutes since midnight, when set.
    pub daily_rebuild_minutes: Option<i32>,
    /// Latest persisted build status: `Some(false)` highlights a failed build.
    pub build_success: Option<bool>,
    /// Build failure message, shown as a warning alert when the row is selected.
    pub build_message: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutDetailItem {
    pub title: String,
    /// Local-time start, formatted for display.
    pub start: String,
    /// Local-time finish, formatted for display.
    pub finish: String,
    /// `GetDisplayDuration`-formatted span.
    pub duration: String,
    /// Serialized scheduling context (drives the Troubleshoot action).
    pub scheduling_context: Option<String>,
    /// `FillerKind` (`-1` for gaps), drives the detail row colouring.
    pub filler_kind: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SequentialPlayoutForm {
    pub id: i64,
    /// "{Number} — {Name}"
    pub channel_label: String,
    /// Raw YAML content (empty string when NULL in DB).
    pub yaml_content: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutCreateChannel {
    pub id: i64,
    pub number: String,
    pub name: String,
    pub playout_count: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutCreateSchedule {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutCreateForm {
    /// Empty string means the upstream default Classic add route.
    pub kind: String,
    pub channel_id: i64,
    pub program_schedule_id: Option<i64>,
    /// External JSON, Sequential YAML, or Scripted Lua file path.
    pub schedule_file: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutDecoGroupOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutDecoOption {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutKindEditorForm {
    pub id: i64,
    pub schedule_kind: i32,
    /// "{Number} — {Name}"
    pub channel_label: String,
    /// `ChannelPlayoutMode`: 0 = `Continuous`, 1 = `OnDemand`.
    pub channel_playout_mode: i32,
    pub schedule_file: String,
    pub default_deco_id: Option<i64>,
    pub deco_groups: Vec<PlayoutDecoGroupOption>,
    pub decos: Vec<PlayoutDecoOption>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutNestedOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutNestedChildOption {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayoutDateFilter {
    pub limit_to_date_range: bool,
    pub start_month: i32,
    pub start_day: i32,
    pub start_year: Option<i32>,
    pub end_month: i32,
    pub end_day: i32,
    pub end_year: Option<i32>,
    pub days_of_week: Vec<i32>,
    pub days_of_month: Vec<i32>,
    pub months_of_year: Vec<i32>,
}

impl Default for PlayoutDateFilter {
    fn default() -> Self {
        Self {
            limit_to_date_range: false,
            start_month: 1,
            start_day: 1,
            start_year: None,
            end_month: 12,
            end_day: 31,
            end_year: None,
            days_of_week: (0..=6).collect(),
            days_of_month: (1..=31).collect(),
            months_of_year: (1..=12).collect(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutAlternateScheduleItemForm {
    pub id: i64,
    pub index: i32,
    pub program_schedule_id: i64,
    pub program_schedule_name: String,
    pub date_filter: PlayoutDateFilter,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutAlternateSchedulesForm {
    pub playout_id: i64,
    pub channel_label: String,
    pub schedules: Vec<PlayoutNestedOption>,
    pub items: Vec<PlayoutAlternateScheduleItemForm>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutTemplateItemForm {
    pub id: i64,
    pub index: i32,
    pub template_group_id: i64,
    pub template_id: i64,
    pub template_name: String,
    pub deco_template_group_id: Option<i64>,
    pub deco_template_id: Option<i64>,
    pub deco_template_name: Option<String>,
    pub date_filter: PlayoutDateFilter,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayoutTemplatesForm {
    pub playout_id: i64,
    pub channel_label: String,
    pub template_groups: Vec<PlayoutNestedOption>,
    pub templates: Vec<PlayoutNestedChildOption>,
    pub deco_template_groups: Vec<PlayoutNestedOption>,
    pub deco_templates: Vec<PlayoutNestedChildOption>,
    pub items: Vec<PlayoutTemplateItemForm>,
}

// ---------------------------------------------------------------------------
// List + detail queries
// ---------------------------------------------------------------------------

/// Paged playouts list. Mirrors `GetPagedPlayoutsHandler`: inner-joins
/// `Channel`, left-joins `ProgramSchedule` and `PlayoutBuildStatus`, filters on
/// `Channel.Name LIKE %query%`, and orders by `Channel.SortNumber`.
#[server]
pub async fn list_playouts(
    query: String,
    page: u32,
    page_size: u32,
) -> Result<Vec<PlayoutListRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let trimmed = query.trim();
    let offset = i64::from(page) * i64::from(page_size.max(1));
    let limit = i64::from(page_size.max(1));

    let base = r#"SELECT p."Id",
                  COALESCE(c."Number", '') AS "ChannelNumber",
                  COALESCE(c."Name",   '') AS "ChannelName",
                  p."ScheduleKind",
                  COALESCE(ps."Name", '') AS "ScheduleName",
                  COALESCE(p."ScheduleFile", '') AS "ScheduleFile",
                  p."DailyRebuildTime",
                  pbs."Success" AS "BuildSuccess",
                  pbs."Message" AS "BuildMessage"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           LEFT JOIN "ProgramSchedule" ps ON ps."Id" = p."ProgramScheduleId"
           LEFT JOIN "PlayoutBuildStatus" pbs ON pbs."PlayoutId" = p."Id""#;

    let rows = if trimmed.is_empty() {
        sqlx::query(&format!(
            r#"{base} ORDER BY c."SortNumber", p."Id" LIMIT ? OFFSET ?"#
        ))
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query(&format!(
            r#"{base} WHERE c."Name" LIKE ? ORDER BY c."SortNumber", p."Id" LIMIT ? OFFSET ?"#
        ))
        .bind(format!("%{trimmed}%"))
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(rows
        .iter()
        .map(|r| {
            let build_success: Option<i64> = r.try_get("BuildSuccess").ok().flatten();
            PlayoutListRow {
                id: r.try_get("Id").unwrap_or(0),
                channel_number: r.try_get("ChannelNumber").unwrap_or_default(),
                channel_name: r.try_get("ChannelName").unwrap_or_default(),
                schedule_kind: r.try_get("ScheduleKind").unwrap_or(0),
                schedule_name: r.try_get("ScheduleName").unwrap_or_default(),
                schedule_file: r.try_get("ScheduleFile").unwrap_or_default(),
                daily_rebuild_minutes: r
                    .try_get::<Option<String>, _>("DailyRebuildTime")
                    .ok()
                    .flatten()
                    .and_then(|v| parse_daily_rebuild_minutes(&v)),
                build_success: build_success.map(|v| v != 0),
                build_message: r.try_get("BuildMessage").ok().flatten(),
            }
        })
        .collect())
}

/// Total playout count. Mirrors `GetPagedPlayoutsHandler` which counts the
/// whole `Playouts` set (the search query filters the page only).
#[server]
pub async fn count_playouts() -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playout""#)
        .fetch_one(&state.pool)
        .await?;
    Ok(u64::try_from(count).unwrap_or(0))
}

#[server]
pub async fn list_playout_create_channels() -> Result<Vec<PlayoutCreateChannel>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_playout_create_channels_with_pool(&state.pool).await
}

#[server]
pub async fn list_playout_create_schedules() -> Result<Vec<PlayoutCreateSchedule>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_playout_create_schedules_with_pool(&state.pool).await
}

/// Paged, future playout items + gaps for the selected playout. Mirrors
/// `GetFuturePlayoutItemsByIdHandler`: items and gaps with `Finish >= now`,
/// optionally hiding filler, combined and ordered by `Start`.
#[server]
pub async fn list_playout_detail_items(
    playout_id: i64,
    show_filler: bool,
    page: u32,
    page_size: u32,
) -> Result<Vec<PlayoutDetailItem>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let now = now_sql();
    let offset = i64::from(page) * i64::from(page_size.max(1));
    let limit = i64::from(page_size.max(1));

    let filler_clause = if show_filler {
        ""
    } else {
        r#" AND pi."FillerKind" = 0"#
    };

    // Window across items + gaps to get the ordered page of rows.
    let window_sql = format!(
        r#"SELECT * FROM (
              SELECT 'item' AS "Kind", pi."Id" AS "Id", pi."Start" AS "Start", pi."Finish" AS "Finish",
                     pi."FillerKind" AS "FillerKind", pi."SchedulingContext" AS "SchedulingContext"
              FROM "PlayoutItem" pi
              WHERE pi."PlayoutId" = ? AND pi."Finish" >= ?{filler_clause}
              UNION ALL
              SELECT 'gap' AS "Kind", pg."Id" AS "Id", pg."Start" AS "Start", pg."Finish" AS "Finish",
                     -1 AS "FillerKind", NULL AS "SchedulingContext"
              FROM "PlayoutGap" pg
              WHERE pg."PlayoutId" = ? AND pg."Finish" >= ?
           ) AS combined
           ORDER BY "Start"
           LIMIT ? OFFSET ?"#
    );

    let window = sqlx::query(&window_sql)
        .bind(playout_id)
        .bind(&now)
        .bind(playout_id)
        .bind(&now)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?;

    // Resolve display titles for the item rows on this page in one query.
    let item_ids: Vec<i64> = window
        .iter()
        .filter(|r| r.try_get::<String, _>("Kind").ok().as_deref() == Some("item"))
        .filter_map(|r| r.try_get::<i64, _>("Id").ok())
        .collect();
    let titles = load_item_titles(&state.pool, &item_ids).await?;

    let mut out = Vec::with_capacity(window.len());
    for r in &window {
        let kind: String = r.try_get("Kind").unwrap_or_default();
        let id: i64 = r.try_get("Id").unwrap_or(0);
        let start_raw: String = r.try_get("Start").unwrap_or_default();
        let finish_raw: String = r.try_get("Finish").unwrap_or_default();
        let duration = display_duration(&start_raw, &finish_raw);
        if kind == "gap" {
            out.push(PlayoutDetailItem {
                title: String::new(), // gap title comes from i18n in the page
                start: format_local(&start_raw),
                finish: format_local(&finish_raw),
                duration,
                scheduling_context: None,
                filler_kind: -1,
            });
        } else {
            let filler_kind =
                i32::try_from(r.try_get::<i64, _>("FillerKind").unwrap_or(0)).unwrap_or(0);
            let ctx: Option<String> = r
                .try_get::<Option<String>, _>("SchedulingContext")
                .ok()
                .flatten()
                .filter(|v| !v.trim().is_empty());
            out.push(PlayoutDetailItem {
                title: titles.get(&id).cloned().unwrap_or_default(),
                start: format_local(&start_raw),
                finish: format_local(&finish_raw),
                duration,
                scheduling_context: ctx,
                filler_kind,
            });
        }
    }

    Ok(out)
}

/// Combined count of future items + gaps for the playout. Mirrors the
/// `combined.CountAsync` in `GetFuturePlayoutItemsByIdHandler`.
#[server]
pub async fn count_playout_detail_items(
    playout_id: i64,
    show_filler: bool,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let now = now_sql();
    let filler_clause = if show_filler {
        ""
    } else {
        r#" AND "FillerKind" = 0"#
    };

    let sql = format!(
        r#"SELECT
              (SELECT COUNT(*) FROM "PlayoutItem"
               WHERE "PlayoutId" = ? AND "Finish" >= ?{filler_clause})
            + (SELECT COUNT(*) FROM "PlayoutGap"
               WHERE "PlayoutId" = ? AND "Finish" >= ?)"#
    );

    let count: i64 = sqlx::query_scalar(&sql)
        .bind(playout_id)
        .bind(&now)
        .bind(playout_id)
        .bind(&now)
        .fetch_one(&state.pool)
        .await?;
    Ok(u64::try_from(count).unwrap_or(0))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Deletes a playout. Mirrors `DeletePlayoutHandler`: removing the `Playout`
/// row cascades to its items/gaps/history/anchors via FK. ErsatzRS serves the
/// channel guide on-the-fly so there is no cache file to delete and no
/// `RefreshChannelList` cache to rewrite.
#[server]
pub async fn delete_playout(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "Playout" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// Enqueues a build for one playout. Mirrors `Playouts.razor.ResetPlayout`:
/// Classic playouts refresh, every other kind resets.
#[allow(clippy::unused_async)]
#[server]
pub async fn reset_playout(id: i64, schedule_kind: i32) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::scheduling::PlayoutBuildMode;

    let state = expect_context::<AppState>();
    let mode = if schedule_kind == KIND_CLASSIC {
        PlayoutBuildMode::Refresh
    } else {
        PlayoutBuildMode::Reset
    };
    enqueue_build(&state, id, mode);
    Ok(())
}

/// Enqueues builds for every eligible playout. Mirrors `ResetAllPlayoutsHandler`:
/// Classic → Refresh, Block/Sequential/Scripted → Reset, ExternalJson/None
/// skipped. Locked playouts are skipped by the scheduler when it dequeues.
#[server]
pub async fn reset_all_playouts() -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::scheduling::PlayoutBuildMode;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(r#"SELECT "Id", "ScheduleKind" FROM "Playout""#)
        .fetch_all(&state.pool)
        .await?;

    for r in &rows {
        let id: i64 = r.try_get("Id").unwrap_or(0);
        let kind: i32 = r.try_get("ScheduleKind").unwrap_or(0);
        let mode = match kind {
            KIND_CLASSIC => PlayoutBuildMode::Refresh,
            KIND_BLOCK | KIND_SEQUENTIAL | KIND_SCRIPTED => PlayoutBuildMode::Reset,
            // ExternalJson / None cannot be reset.
            _ => continue,
        };
        enqueue_build(&state, id, mode);
    }
    Ok(())
}

/// Persists a playout's `DailyRebuildTime`. Mirrors `UpdatePlayoutHandler`:
/// `None` clears it, otherwise it stores `HH:MM:SS`.
#[server]
pub async fn save_playout_daily_reset(id: i64, minutes: Option<i32>) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let value = minutes.map(format_daily_rebuild_time);
    sqlx::query(r#"UPDATE "Playout" SET "DailyRebuildTime" = ? WHERE "Id" = ?"#)
        .bind(value)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// Updates an external-json playout's `ScheduleFile`. Mirrors
/// `UpdateExternalJsonPlayoutHandler` (without the on-the-fly guide cache that
/// ErsatzRS does not keep).
#[server]
pub async fn update_external_json_playout(
    id: i64,
    schedule_file: String,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleFile" = ? WHERE "Id" = ? AND "ScheduleKind" = ?"#)
        .bind(&schedule_file)
        .bind(id)
        .bind(KIND_EXTERNAL_JSON)
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// Creates a playout from the add page. Ports the `Create*PlayoutHandler`
/// family: validate channel ownership, validate the selected schedule/file,
/// insert the `Playout`, then queue an initial reset build.
#[server]
pub async fn create_playout(form: PlayoutCreateForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    create_playout_with_state(&state, form).await
}

/// Snapshot of playout ids currently building. Mirrors the admin UI polling
/// `IEntityLocker.IsPlayoutLocked`.
#[allow(clippy::unused_async)]
#[server]
pub async fn list_locked_playouts() -> Result<Vec<i64>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.playout_locker.locked_playouts())
}

/// Reads the persisted `Show Filler` detail preference
/// (`pages.playouts.detail_show_filler`).
#[server]
pub async fn get_show_filler_pref() -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::repositories::ConfigRepository;

    let state = expect_context::<AppState>();
    let value = state
        .repositories
        .config
        .get("pages.playouts.detail_show_filler")
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(value.is_some_and(|v| v.trim().eq_ignore_ascii_case("true")))
}

/// Persists the `Show Filler` detail preference.
#[server]
pub async fn set_show_filler_pref(show_filler: bool) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::repositories::ConfigRepository;

    let state = expect_context::<AppState>();
    state
        .repositories
        .config
        .set(
            "pages.playouts.detail_show_filler",
            if show_filler { "true" } else { "false" },
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(())
}

/// Enriches a serialized scheduling context. Mirrors
/// `ProcessSchedulingContextHandler`: Classic (`ScheduleId`) and Block
/// (`BlockId`) contexts get human-readable names; anything else is
/// pretty-printed (or returned as-is when it is not valid JSON).
#[server]
pub async fn process_scheduling_context(serialized: String) -> Result<String, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(enrich_scheduling_context(&state.pool, &serialized).await)
}

// ---------------------------------------------------------------------------
// Classic / Block / Scripted top-level editors.
// ---------------------------------------------------------------------------

/// Loads one of the top-level Classic, Block, or Scripted editor forms.
#[server]
pub async fn get_playout_kind_editor(
    id: i64,
    schedule_kind: i32,
) -> Result<Option<PlayoutKindEditorForm>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    if !matches!(schedule_kind, KIND_CLASSIC | KIND_BLOCK | KIND_SCRIPTED) {
        return Ok(None);
    }

    let row = sqlx::query(
        r#"SELECT p."Id",
                  p."ScheduleKind",
                  COALESCE(c."Number", '') AS "ChannelNumber",
                  COALESCE(c."Name", '') AS "ChannelName",
                  COALESCE(c."PlayoutMode", 0) AS "ChannelPlayoutMode",
                  COALESCE(p."ScheduleFile", '') AS "ScheduleFile",
                  p."DecoId"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE p."Id" = ? AND p."ScheduleKind" = ?"#,
    )
    .bind(id)
    .bind(schedule_kind)
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let number: String = row.try_get("ChannelNumber").unwrap_or_default();
    let name: String = row.try_get("ChannelName").unwrap_or_default();
    let (deco_groups, decos) = if schedule_kind == KIND_BLOCK {
        load_playout_deco_options(&state.pool).await?
    } else {
        (Vec::new(), Vec::new())
    };

    Ok(Some(PlayoutKindEditorForm {
        id: row.try_get("Id").unwrap_or(0),
        schedule_kind: row.try_get("ScheduleKind").unwrap_or(0),
        channel_label: format!("{number} \u{2014} {name}"),
        channel_playout_mode: row.try_get("ChannelPlayoutMode").unwrap_or(0),
        schedule_file: row.try_get("ScheduleFile").unwrap_or_default(),
        default_deco_id: row.try_get::<Option<i64>, _>("DecoId").ok().flatten(),
        deco_groups,
        decos,
    }))
}

/// Persists a Scripted playout schedule file/command line and refreshes the
/// channel through the scheduler worker.
#[server]
pub async fn save_scripted_playout(id: i64, schedule_file: String) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::scheduling::PlayoutBuildMode;

    let state = expect_context::<AppState>();
    validate_scripted_schedule_file(&schedule_file)?;

    let result = sqlx::query(
        r#"UPDATE "Playout" SET "ScheduleFile" = ? WHERE "Id" = ? AND "ScheduleKind" = ?"#,
    )
    .bind(schedule_file.trim())
    .bind(id)
    .bind(KIND_SCRIPTED)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ServerFnError::new("Playout does not exist."));
    }

    enqueue_build(&state, id, PlayoutBuildMode::Reset);
    Ok(())
}

#[server]
pub async fn save_playout_default_deco(id: i64, deco_id: Option<i64>) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    if let Some(deco_id) = deco_id.filter(|id| *id > 0) {
        let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Deco" WHERE "Id" = ?"#)
            .bind(deco_id)
            .fetch_one(&state.pool)
            .await?;
        if exists == 0 {
            return Err(ServerFnError::new("Deco does not exist."));
        }
    }

    sqlx::query(r#"UPDATE "Playout" SET "DecoId" = ? WHERE "Id" = ? AND "ScheduleKind" = ?"#)
        .bind(deco_id.filter(|id| *id > 0))
        .bind(id)
        .bind(KIND_BLOCK)
        .execute(&state.pool)
        .await?;

    Ok(())
}

/// Loads the Classic nested alternate schedule editor. ErsatzTV appends the
/// current default schedule as the final catch-all row; this does the same.
#[server]
pub async fn get_playout_alternate_schedules(
    id: i64,
) -> Result<Option<PlayoutAlternateSchedulesForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    get_playout_alternate_schedules_with_pool(&state.pool, id).await
}

#[server]
pub async fn save_playout_alternate_schedules(
    playout_id: i64,
    items: Vec<PlayoutAlternateScheduleItemForm>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::scheduling::PlayoutBuildMode;

    let state = expect_context::<AppState>();
    save_playout_alternate_schedules_with_pool(&state.pool, playout_id, &items).await?;

    // ErsatzTV's `ReplacePlayoutAlternateScheduleItemsHandler` only refreshes
    // when an existing playout window's effective schedule actually changes,
    // computed per-day via `AlternateScheduleSelector`. We have not ported that
    // selector yet, so we conservatively refresh whenever the playout already
    // has items — a safe superset of ErsatzTV's cases (a refresh is
    // idempotent). When the playout has no items there is nothing to refresh,
    // matching ErsatzTV's `maybeLastPlayoutItem` guard. See ParityDebt in
    // plans/playouts-parity-slice.md.
    if playout_has_items(&state.pool, playout_id).await? {
        enqueue_build(&state, playout_id, PlayoutBuildMode::Refresh);
    }
    Ok(())
}

/// Loads the Block nested template editor.
#[server]
pub async fn get_playout_templates(id: i64) -> Result<Option<PlayoutTemplatesForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    get_playout_templates_with_pool(&state.pool, id).await
}

#[server]
pub async fn save_playout_templates(
    playout_id: i64,
    items: Vec<PlayoutTemplateItemForm>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_playout_templates_with_pool(&state.pool, playout_id, &items).await
}

/// Equivalent to ErsatzTV's `ErasePlayoutItems`: remove generated items only.
#[server]
pub async fn erase_playout_items(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Sequential (YAML) editor — unchanged from the Phase 6 slice.
// ---------------------------------------------------------------------------

/// Loads a Sequential (YAML) playout (`ScheduleKind = 3`) by id.
#[server]
pub async fn get_sequential_playout(
    id: i64,
) -> Result<Option<SequentialPlayoutForm>, ServerFnError> {
    use crate::host::state::AppState;
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
/// background playout rebuild so scheduled items stay current.
#[server]
pub async fn save_sequential_playout(id: i64, yaml_content: String) -> Result<(), ServerFnError> {
    use std::sync::Arc;

    use crate::host::state::AppState;

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

/// Runs a reset build for a Sequential (YAML) playout in the background.
#[cfg(feature = "ssr")]
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
#[server]
pub async fn erase_playout_items_and_history(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let mut tx = state.pool.begin().await?;

    sqlx::query(
        r#"UPDATE "Playout"
           SET "Seed" = (SELECT random()), "OnDemandCheckpoint" = NULL
           WHERE "Id" = ?"#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(r#"DELETE FROM "PlayoutAnchor" WHERE "PlayoutId" = ?"#)
        .bind(id)
        .execute(&mut *tx)
        .await?;

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
#[allow(clippy::unused_async)]
#[server]
pub async fn validate_yaml_playout(content: String) -> Result<(), ServerFnError> {
    use ersatzrs_core::scheduling::YamlPlayoutEngine;
    match YamlPlayoutEngine::definition_from_str(&content) {
        Ok(_) => Ok(()),
        Err(e) => Err(ServerFnError::new(e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// SSR-only helpers
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
fn enqueue_build(
    state: &crate::host::state::AppState,
    playout_id: i64,
    mode: ersatzrs_core::scheduling::PlayoutBuildMode,
) {
    let request = ersatzrs_core::scheduling::PlayoutBuildRequest { playout_id, mode };
    if let Err(error) = state.playout_build_tx.try_send(request) {
        tracing::warn!(
            playout_id,
            ?mode,
            %error,
            "dropping requested playout build; the periodic scheduler will retry later"
        );
    }
}

/// Whether the playout already has any generated items. Mirrors ErsatzTV's
/// `maybeLastPlayoutItem` guard: with no items there is nothing to refresh.
#[cfg(feature = "ssr")]
async fn playout_has_items(
    pool: &ersatzrs_infra::db::DbPool,
    playout_id: i64,
) -> Result<bool, ServerFnError> {
    let exists: Option<i64> =
        sqlx::query_scalar(r#"SELECT 1 FROM "PlayoutItem" WHERE "PlayoutId" = ? LIMIT 1"#)
            .bind(playout_id)
            .fetch_optional(pool)
            .await?;
    Ok(exists.is_some())
}

#[cfg(feature = "ssr")]
pub async fn create_playout_with_state(
    state: &crate::host::state::AppState,
    form: PlayoutCreateForm,
) -> Result<i64, ServerFnError> {
    use ersatzrs_core::scheduling::PlayoutBuildMode;

    let id = create_playout_with_pool(&state.pool, &form).await?;
    enqueue_build(state, id, PlayoutBuildMode::Reset);
    Ok(id)
}

#[cfg(feature = "ssr")]
async fn list_playout_create_channels_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<PlayoutCreateChannel>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT c."Id",
                  COALESCE(c."Number", '') AS "Number",
                  COALESCE(c."Name", '') AS "Name",
                  COUNT(p."Id") AS "PlayoutCount"
           FROM "Channel" c
           LEFT JOIN "Playout" p ON p."ChannelId" = c."Id"
           GROUP BY c."Id", c."Number", c."Name", c."SortNumber"
           ORDER BY c."SortNumber", c."Id""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| PlayoutCreateChannel {
            id: r.try_get("Id").unwrap_or(0),
            number: r.try_get("Number").unwrap_or_default(),
            name: r.try_get("Name").unwrap_or_default(),
            playout_count: r.try_get("PlayoutCount").unwrap_or(0),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn list_playout_create_schedules_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<PlayoutCreateSchedule>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "ProgramSchedule"
           ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| PlayoutCreateSchedule {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
pub async fn create_playout_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    form: &PlayoutCreateForm,
) -> Result<i64, ServerFnError> {
    use sqlx::Row;

    let kind = parse_create_kind(&form.kind)?;
    let channel_id = form.channel_id;
    if channel_id <= 0 {
        return Err(ServerFnError::new("Channel does not exist"));
    }

    let channel = sqlx::query(
        r#"SELECT c."Id",
                  c."PlayoutMode",
                  COUNT(p."Id") AS "PlayoutCount"
           FROM "Channel" c
           LEFT JOIN "Playout" p ON p."ChannelId" = c."Id"
           WHERE c."Id" = ?
           GROUP BY c."Id", c."PlayoutMode""#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await?;

    let Some(channel) = channel else {
        return Err(ServerFnError::new("Channel does not exist"));
    };

    let playout_count: i64 = channel.try_get("PlayoutCount").unwrap_or(0);
    if playout_count > 0 {
        return Err(ServerFnError::new("Channel already has one playout"));
    }

    let mut program_schedule_id = None;
    let mut schedule_file = None;

    match kind {
        KIND_CLASSIC => {
            let Some(schedule_id) = form.program_schedule_id.filter(|id| *id > 0) else {
                return Err(ServerFnError::new("Program schedule does not exist"));
            };
            let schedule = sqlx::query(
                r#"SELECT ps."Id",
                          COUNT(psi."Id") AS "ItemCount"
                   FROM "ProgramSchedule" ps
                   LEFT JOIN "ProgramScheduleItem" psi ON psi."ProgramScheduleId" = ps."Id"
                   WHERE ps."Id" = ?
                   GROUP BY ps."Id""#,
            )
            .bind(schedule_id)
            .fetch_optional(pool)
            .await?;
            let Some(schedule) = schedule else {
                return Err(ServerFnError::new("Program schedule does not exist"));
            };
            let item_count: i64 = schedule.try_get("ItemCount").unwrap_or(0);
            if item_count == 0 {
                return Err(ServerFnError::new("Program schedule must have items"));
            }
            program_schedule_id = Some(schedule_id);
        }
        KIND_BLOCK => {}
        KIND_SEQUENTIAL | KIND_EXTERNAL_JSON => {
            let file = form.schedule_file.trim();
            if file.is_empty() || std::fs::metadata(file).is_err() {
                return Err(ServerFnError::new(schedule_file_missing_message(kind)));
            }
            schedule_file = Some(file.to_owned());
        }
        KIND_SCRIPTED => {
            let file = form.schedule_file.trim();
            let script_file = first_command_token(file);
            if script_file.is_empty() || std::fs::metadata(&script_file).is_err() {
                return Err(ServerFnError::new(schedule_file_missing_message(kind)));
            }
            schedule_file = Some(file.to_owned());
        }
        _ => return Err(ServerFnError::new("Unsupported playout kind")),
    }

    // ErsatzTV seeds with `new Random().Next()` (non-negative 32-bit int).
    // Generate it in Rust and bind it so the INSERT stays backend-neutral; an
    // embedded `random()` would be SQLite-only and overflow MySQL's `INT`.
    let seed = next_seed();

    sqlx::query(
        r#"INSERT INTO "Playout"
           ("ChannelId", "ProgramScheduleId", "ScheduleFile", "ScheduleKind", "Seed")
           VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(channel_id)
    .bind(program_schedule_id)
    .bind(schedule_file)
    .bind(kind)
    .bind(seed)
    .execute(pool)
    .await?;

    Ok(sqlx::query_scalar(
        r#"SELECT "Id"
           FROM "Playout"
           WHERE "ChannelId" = ?
           ORDER BY "Id" DESC"#,
    )
    .bind(channel_id)
    .fetch_one(pool)
    .await?)
}

/// A non-negative 32-bit playout seed, mirroring ErsatzTV's `new Random().Next()`.
///
/// The scheduling seed only needs per-playout variety, not cryptographic
/// quality, so we derive it from a high-resolution timestamp rather than
/// pulling in a `rand` dependency. The value is masked into `0..=i32::MAX`
/// to fit the `Seed INT` column on every backend.
#[cfg(feature = "ssr")]
fn next_seed() -> i32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    // Hash the timestamp with a process-randomized hasher so concurrent
    // creates in the same nanosecond bucket still diverge.
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(nanos);
    // Masking with `i32::MAX` guarantees the result fits `0..=i32::MAX`, so the
    // `try_from` never fails.
    i32::try_from(hasher.finish() & u64::try_from(i32::MAX).unwrap_or(u64::MAX)).unwrap_or(0)
}

#[cfg(feature = "ssr")]
async fn load_playout_deco_options(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<(Vec<PlayoutDecoGroupOption>, Vec<PlayoutDecoOption>), ServerFnError> {
    use sqlx::Row;

    let group_rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "DecoGroup"
           ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await?;

    let deco_rows = sqlx::query(
        r#"SELECT "Id", "DecoGroupId", COALESCE("Name", '') AS "Name"
           FROM "Deco"
           ORDER BY "DecoGroupId", "Name", "Id""#,
    )
    .fetch_all(pool)
    .await?;

    let groups = group_rows
        .iter()
        .map(|row| PlayoutDecoGroupOption {
            id: row.try_get("Id").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect();
    let decos = deco_rows
        .iter()
        .map(|row| PlayoutDecoOption {
            id: row.try_get("Id").unwrap_or_default(),
            group_id: row.try_get("DecoGroupId").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect();

    Ok((groups, decos))
}

#[cfg(feature = "ssr")]
async fn get_playout_alternate_schedules_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    playout_id: i64,
) -> Result<Option<PlayoutAlternateSchedulesForm>, ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT p."Id",
                  p."ProgramScheduleId",
                  COALESCE(c."Number", '') AS "ChannelNumber",
                  COALESCE(c."Name", '') AS "ChannelName"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE p."Id" = ? AND p."ScheduleKind" = ?"#,
    )
    .bind(playout_id)
    .bind(KIND_CLASSIC)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let schedules = load_nested_options(pool, "ProgramSchedule").await?;
    let schedule_name = schedules
        .iter()
        .map(|schedule| (schedule.id, schedule.name.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    let alternate_rows = sqlx::query(
        r#"SELECT "Id", "ProgramScheduleId", "Index", "DaysOfWeek", "DaysOfMonth",
                  "MonthsOfYear", "LimitToDateRange", "StartMonth", "StartDay",
                  "StartYear", "EndMonth", "EndDay", "EndYear"
           FROM "ProgramScheduleAlternate"
           WHERE "PlayoutId" = ?
           ORDER BY "Index", "Id""#,
    )
    .bind(playout_id)
    .fetch_all(pool)
    .await?;

    let mut items = alternate_rows
        .iter()
        .map(|row| alternate_schedule_item_from_row(row, &schedule_name))
        .collect::<Vec<_>>();

    let default_schedule_id = row
        .try_get::<Option<i64>, _>("ProgramScheduleId")
        .ok()
        .flatten()
        .unwrap_or(0);
    if default_schedule_id > 0 {
        let index = items.iter().map(|item| item.index).max().unwrap_or(0) + 1;
        items.push(PlayoutAlternateScheduleItemForm {
            id: -1,
            index,
            program_schedule_id: default_schedule_id,
            program_schedule_name: schedule_name
                .get(&default_schedule_id)
                .cloned()
                .unwrap_or_default(),
            date_filter: PlayoutDateFilter::default(),
        });
    }

    let number: String = row.try_get("ChannelNumber").unwrap_or_default();
    let name: String = row.try_get("ChannelName").unwrap_or_default();
    Ok(Some(PlayoutAlternateSchedulesForm {
        playout_id,
        channel_label: format!("{number} \u{2014} {name}"),
        schedules,
        items,
    }))
}

#[cfg(feature = "ssr")]
async fn save_playout_alternate_schedules_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    playout_id: i64,
    items: &[PlayoutAlternateScheduleItemForm],
) -> Result<(), ServerFnError> {
    use sqlx::Row;

    if items.is_empty() {
        return Err(ServerFnError::new(
            "Alternate schedules must include a default schedule.",
        ));
    }

    let exists: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Playout" WHERE "Id" = ? AND "ScheduleKind" = ?"#,
    )
    .bind(playout_id)
    .bind(KIND_CLASSIC)
    .fetch_one(pool)
    .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Playout does not exist."));
    }

    let mut ordered = items.to_vec();
    ordered.sort_by_key(|item| item.index);
    for (position, item) in ordered.iter_mut().enumerate() {
        item.index = i32::try_from(position + 1).unwrap_or(i32::MAX);
    }
    let default = ordered.last().ok_or_else(|| {
        ServerFnError::new("Alternate schedules must include a default schedule.")
    })?;
    validate_program_schedule_ids(pool, ordered.iter().map(|item| item.program_schedule_id))
        .await?;

    let saved_ids = ordered
        .iter()
        .take(ordered.len().saturating_sub(1))
        .filter_map(|item| (item.id > 0).then_some(item.id))
        .collect::<Vec<_>>();
    let existing_rows =
        sqlx::query(r#"SELECT "Id" FROM "ProgramScheduleAlternate" WHERE "PlayoutId" = ?"#)
            .bind(playout_id)
            .fetch_all(pool)
            .await?;

    let mut tx = pool.begin().await?;
    for row in existing_rows {
        let id: i64 = row.try_get("Id").unwrap_or_default();
        if !saved_ids.contains(&id) {
            sqlx::query(r#"DELETE FROM "ProgramScheduleAlternate" WHERE "Id" = ?"#)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }

    for item in ordered.iter().take(ordered.len().saturating_sub(1)) {
        if item.id > 0 {
            sqlx::query(
                r#"UPDATE "ProgramScheduleAlternate"
                   SET "ProgramScheduleId" = ?, "Index" = ?, "DaysOfWeek" = ?,
                       "DaysOfMonth" = ?, "MonthsOfYear" = ?, "LimitToDateRange" = ?,
                       "StartMonth" = ?, "StartDay" = ?, "StartYear" = ?,
                       "EndMonth" = ?, "EndDay" = ?, "EndYear" = ?
                   WHERE "Id" = ? AND "PlayoutId" = ?"#,
            )
            .bind(item.program_schedule_id)
            .bind(item.index)
            .bind(encode_day_names(&item.date_filter.days_of_week))
            .bind(encode_int_csv(&item.date_filter.days_of_month))
            .bind(encode_int_csv(&item.date_filter.months_of_year))
            .bind(item.date_filter.limit_to_date_range)
            .bind(item.date_filter.start_month)
            .bind(item.date_filter.start_day)
            .bind(item.date_filter.start_year)
            .bind(item.date_filter.end_month)
            .bind(item.date_filter.end_day)
            .bind(item.date_filter.end_year)
            .bind(item.id)
            .bind(playout_id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                r#"INSERT INTO "ProgramScheduleAlternate"
                   ("PlayoutId", "ProgramScheduleId", "Index", "DaysOfWeek", "DaysOfMonth",
                    "MonthsOfYear", "LimitToDateRange", "StartMonth", "StartDay",
                    "StartYear", "EndMonth", "EndDay", "EndYear")
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(playout_id)
            .bind(item.program_schedule_id)
            .bind(item.index)
            .bind(encode_day_names(&item.date_filter.days_of_week))
            .bind(encode_int_csv(&item.date_filter.days_of_month))
            .bind(encode_int_csv(&item.date_filter.months_of_year))
            .bind(item.date_filter.limit_to_date_range)
            .bind(item.date_filter.start_month)
            .bind(item.date_filter.start_day)
            .bind(item.date_filter.start_year)
            .bind(item.date_filter.end_month)
            .bind(item.date_filter.end_day)
            .bind(item.date_filter.end_year)
            .execute(&mut *tx)
            .await?;
        }
    }

    sqlx::query(r#"UPDATE "Playout" SET "ProgramScheduleId" = ? WHERE "Id" = ?"#)
        .bind(default.program_schedule_id)
        .bind(playout_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

#[cfg(feature = "ssr")]
async fn get_playout_templates_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    playout_id: i64,
) -> Result<Option<PlayoutTemplatesForm>, ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT p."Id",
                  COALESCE(c."Number", '') AS "ChannelNumber",
                  COALESCE(c."Name", '') AS "ChannelName"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE p."Id" = ? AND p."ScheduleKind" = ?"#,
    )
    .bind(playout_id)
    .bind(KIND_BLOCK)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let template_groups = load_nested_options(pool, "TemplateGroup").await?;
    let templates = load_nested_child_options(pool, "Template", "TemplateGroupId").await?;
    let deco_template_groups = load_nested_options(pool, "DecoTemplateGroup").await?;
    let deco_templates =
        load_nested_child_options(pool, "DecoTemplate", "DecoTemplateGroupId").await?;

    let rows = sqlx::query(
        r#"SELECT pt."Id", pt."Index", pt."TemplateId", t."TemplateGroupId",
                  COALESCE(t."Name", '') AS "TemplateName",
                  pt."DecoTemplateId", dt."DecoTemplateGroupId",
                  dt."Name" AS "DecoTemplateName",
                  pt."DaysOfWeek", pt."DaysOfMonth", pt."MonthsOfYear",
                  pt."LimitToDateRange", pt."StartMonth", pt."StartDay", pt."StartYear",
                  pt."EndMonth", pt."EndDay", pt."EndYear"
           FROM "PlayoutTemplate" pt
           JOIN "Template" t ON t."Id" = pt."TemplateId"
           LEFT JOIN "DecoTemplate" dt ON dt."Id" = pt."DecoTemplateId"
           WHERE pt."PlayoutId" = ?
           ORDER BY pt."Index", pt."Id""#,
    )
    .bind(playout_id)
    .fetch_all(pool)
    .await?;

    let items = rows
        .iter()
        .map(|row| PlayoutTemplateItemForm {
            id: row.try_get("Id").unwrap_or_default(),
            index: row.try_get("Index").unwrap_or_default(),
            template_group_id: row.try_get("TemplateGroupId").unwrap_or_default(),
            template_id: row.try_get("TemplateId").unwrap_or_default(),
            template_name: row.try_get("TemplateName").unwrap_or_default(),
            deco_template_group_id: row.try_get("DecoTemplateGroupId").ok().flatten(),
            deco_template_id: row.try_get("DecoTemplateId").ok().flatten(),
            deco_template_name: row.try_get("DecoTemplateName").ok().flatten(),
            date_filter: date_filter_from_row(row, false),
        })
        .collect();

    let number: String = row.try_get("ChannelNumber").unwrap_or_default();
    let name: String = row.try_get("ChannelName").unwrap_or_default();
    Ok(Some(PlayoutTemplatesForm {
        playout_id,
        channel_label: format!("{number} \u{2014} {name}"),
        template_groups,
        templates,
        deco_template_groups,
        deco_templates,
        items,
    }))
}

#[cfg(feature = "ssr")]
async fn save_playout_templates_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    playout_id: i64,
    items: &[PlayoutTemplateItemForm],
) -> Result<(), ServerFnError> {
    use sqlx::Row;

    let exists: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Playout" WHERE "Id" = ? AND "ScheduleKind" = ?"#,
    )
    .bind(playout_id)
    .bind(KIND_BLOCK)
    .fetch_one(pool)
    .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Playout does not exist."));
    }
    if items.iter().any(|item| item.template_id <= 0) {
        return Err(ServerFnError::new(
            "Unable to save; item has no template selected.",
        ));
    }
    validate_template_ids(pool, items.iter().map(|item| item.template_id)).await?;
    validate_deco_template_ids(pool, items.iter().filter_map(|item| item.deco_template_id)).await?;

    let mut ordered = items.to_vec();
    ordered.sort_by_key(|item| item.index);
    for (position, item) in ordered.iter_mut().enumerate() {
        item.index = i32::try_from(position + 1).unwrap_or(i32::MAX);
    }

    let saved_ids = ordered
        .iter()
        .filter_map(|item| (item.id > 0).then_some(item.id))
        .collect::<Vec<_>>();
    let existing_rows = sqlx::query(r#"SELECT "Id" FROM "PlayoutTemplate" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .fetch_all(pool)
        .await?;

    let mut tx = pool.begin().await?;
    for row in existing_rows {
        let id: i64 = row.try_get("Id").unwrap_or_default();
        if !saved_ids.contains(&id) {
            sqlx::query(r#"DELETE FROM "PlayoutTemplate" WHERE "Id" = ?"#)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }

    for item in &ordered {
        let deco_template_id = item.deco_template_id.filter(|id| *id > 0);
        if item.id > 0 {
            sqlx::query(
                r#"UPDATE "PlayoutTemplate"
                   SET "DateUpdated" = CURRENT_TIMESTAMP, "TemplateId" = ?,
                       "DecoTemplateId" = ?, "Index" = ?, "DaysOfWeek" = ?,
                       "DaysOfMonth" = ?, "MonthsOfYear" = ?, "LimitToDateRange" = ?,
                       "StartMonth" = ?, "StartDay" = ?, "StartYear" = ?,
                       "EndMonth" = ?, "EndDay" = ?, "EndYear" = ?
                   WHERE "Id" = ? AND "PlayoutId" = ?"#,
            )
            .bind(item.template_id)
            .bind(deco_template_id)
            .bind(item.index)
            .bind(encode_int_json(&item.date_filter.days_of_week))
            .bind(encode_int_json(&item.date_filter.days_of_month))
            .bind(encode_int_json(&item.date_filter.months_of_year))
            .bind(item.date_filter.limit_to_date_range)
            .bind(item.date_filter.start_month)
            .bind(item.date_filter.start_day)
            .bind(item.date_filter.start_year)
            .bind(item.date_filter.end_month)
            .bind(item.date_filter.end_day)
            .bind(item.date_filter.end_year)
            .bind(item.id)
            .bind(playout_id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                r#"INSERT INTO "PlayoutTemplate"
                   ("DateUpdated", "TemplateId", "DecoTemplateId", "Index", "DaysOfWeek",
                    "DaysOfMonth", "MonthsOfYear", "LimitToDateRange", "StartMonth",
                    "StartDay", "StartYear", "EndMonth", "EndDay", "EndYear", "PlayoutId")
                   VALUES (CURRENT_TIMESTAMP, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(item.template_id)
            .bind(deco_template_id)
            .bind(item.index)
            .bind(encode_int_json(&item.date_filter.days_of_week))
            .bind(encode_int_json(&item.date_filter.days_of_month))
            .bind(encode_int_json(&item.date_filter.months_of_year))
            .bind(item.date_filter.limit_to_date_range)
            .bind(item.date_filter.start_month)
            .bind(item.date_filter.start_day)
            .bind(item.date_filter.start_year)
            .bind(item.date_filter.end_month)
            .bind(item.date_filter.end_day)
            .bind(item.date_filter.end_year)
            .bind(playout_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

#[cfg(feature = "ssr")]
async fn load_nested_options(
    pool: &ersatzrs_infra::db::DbPool,
    table: &'static str,
) -> Result<Vec<PlayoutNestedOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(&format!(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "{table}" ORDER BY "Name", "Id""#
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| PlayoutNestedOption {
            id: row.try_get("Id").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_nested_child_options(
    pool: &ersatzrs_infra::db::DbPool,
    table: &'static str,
    group_column: &'static str,
) -> Result<Vec<PlayoutNestedChildOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(&format!(
        r#"SELECT "Id", "{group_column}", COALESCE("Name", '') AS "Name"
           FROM "{table}"
           ORDER BY "{group_column}", "Name", "Id""#
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| PlayoutNestedChildOption {
            id: row.try_get("Id").unwrap_or_default(),
            group_id: row.try_get(group_column).unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
fn alternate_schedule_item_from_row(
    row: &ersatzrs_infra::db::DbRow,
    schedule_name: &std::collections::HashMap<i64, String>,
) -> PlayoutAlternateScheduleItemForm {
    use sqlx::Row;

    let schedule_id = row.try_get("ProgramScheduleId").unwrap_or_default();
    PlayoutAlternateScheduleItemForm {
        id: row.try_get("Id").unwrap_or_default(),
        index: row.try_get("Index").unwrap_or_default(),
        program_schedule_id: schedule_id,
        program_schedule_name: schedule_name.get(&schedule_id).cloned().unwrap_or_default(),
        date_filter: date_filter_from_row(row, true),
    }
}

#[cfg(feature = "ssr")]
fn date_filter_from_row(
    row: &ersatzrs_infra::db::DbRow,
    alternate_day_names: bool,
) -> PlayoutDateFilter {
    use sqlx::Row;

    let days_of_week = if alternate_day_names {
        decode_day_names(
            row.try_get::<Option<String>, _>("DaysOfWeek")
                .ok()
                .flatten(),
        )
    } else {
        decode_int_json(
            row.try_get::<Option<String>, _>("DaysOfWeek")
                .ok()
                .flatten(),
        )
    };

    PlayoutDateFilter {
        limit_to_date_range: row.try_get::<bool, _>("LimitToDateRange").unwrap_or(false),
        start_month: row.try_get("StartMonth").unwrap_or(1),
        start_day: row.try_get("StartDay").unwrap_or(1),
        start_year: row.try_get("StartYear").ok().flatten(),
        end_month: row.try_get("EndMonth").unwrap_or(12),
        end_day: row.try_get("EndDay").unwrap_or(31),
        end_year: row.try_get("EndYear").ok().flatten(),
        days_of_week: normalize_days_of_week(days_of_week),
        days_of_month: normalize_days_of_month(decode_int_list(
            row.try_get::<Option<String>, _>("DaysOfMonth")
                .ok()
                .flatten(),
        )),
        months_of_year: normalize_months(decode_int_list(
            row.try_get::<Option<String>, _>("MonthsOfYear")
                .ok()
                .flatten(),
        )),
    }
}

#[cfg(feature = "ssr")]
async fn validate_program_schedule_ids(
    pool: &ersatzrs_infra::db::DbPool,
    ids: impl Iterator<Item = i64>,
) -> Result<(), ServerFnError> {
    validate_ids(
        pool,
        "ProgramSchedule",
        ids,
        "Program schedule does not exist.",
    )
    .await
}

#[cfg(feature = "ssr")]
async fn validate_template_ids(
    pool: &ersatzrs_infra::db::DbPool,
    ids: impl Iterator<Item = i64>,
) -> Result<(), ServerFnError> {
    validate_ids(pool, "Template", ids, "Template does not exist.").await
}

#[cfg(feature = "ssr")]
async fn validate_deco_template_ids(
    pool: &ersatzrs_infra::db::DbPool,
    ids: impl Iterator<Item = i64>,
) -> Result<(), ServerFnError> {
    validate_ids(pool, "DecoTemplate", ids, "Deco template does not exist.").await
}

#[cfg(feature = "ssr")]
async fn validate_ids(
    pool: &ersatzrs_infra::db::DbPool,
    table: &'static str,
    ids: impl Iterator<Item = i64>,
    message: &'static str,
) -> Result<(), ServerFnError> {
    let mut unique = ids.filter(|id| *id > 0).collect::<Vec<_>>();
    unique.sort_unstable();
    unique.dedup();
    for id in unique {
        let exists: i64 =
            sqlx::query_scalar(&format!(r#"SELECT COUNT(*) FROM "{table}" WHERE "Id" = ?"#))
                .bind(id)
                .fetch_one(pool)
                .await?;
        if exists == 0 {
            return Err(ServerFnError::new(message));
        }
    }
    Ok(())
}

#[cfg(feature = "ssr")]
fn encode_day_names(values: &[i32]) -> String {
    let names = normalize_days_of_week(values.to_vec())
        .into_iter()
        .filter_map(day_name)
        .collect::<Vec<_>>();
    serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_owned())
}

#[cfg(feature = "ssr")]
fn day_name(value: i32) -> Option<&'static str> {
    match value {
        0 => Some("Sunday"),
        1 => Some("Monday"),
        2 => Some("Tuesday"),
        3 => Some("Wednesday"),
        4 => Some("Thursday"),
        5 => Some("Friday"),
        6 => Some("Saturday"),
        _ => None,
    }
}

#[cfg(feature = "ssr")]
fn decode_day_names(value: Option<String>) -> Vec<i32> {
    let Some(value) = value else {
        return (0..=6).collect();
    };
    serde_json::from_str::<Vec<String>>(&value)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|name| match name.as_str() {
            "Sunday" => Some(0),
            "Monday" => Some(1),
            "Tuesday" => Some(2),
            "Wednesday" => Some(3),
            "Thursday" => Some(4),
            "Friday" => Some(5),
            "Saturday" => Some(6),
            _ => None,
        })
        .collect()
}

#[cfg(feature = "ssr")]
fn decode_int_list(value: Option<String>) -> Vec<i32> {
    let Some(value) = value else {
        return Vec::new();
    };
    if value.trim_start().starts_with('[') {
        decode_int_json(Some(value))
    } else {
        value
            .split(',')
            .filter_map(|part| part.trim().parse::<i32>().ok())
            .collect()
    }
}

#[cfg(feature = "ssr")]
fn decode_int_json(value: Option<String>) -> Vec<i32> {
    value
        .and_then(|value| serde_json::from_str::<Vec<i32>>(&value).ok())
        .unwrap_or_default()
}

#[cfg(feature = "ssr")]
fn encode_int_csv(values: &[i32]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(feature = "ssr")]
fn encode_int_json(values: &[i32]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

#[cfg(feature = "ssr")]
fn normalize_days_of_week(values: Vec<i32>) -> Vec<i32> {
    normalize_range(values, 0, 6)
}

#[cfg(feature = "ssr")]
fn normalize_days_of_month(values: Vec<i32>) -> Vec<i32> {
    normalize_range(values, 1, 31)
}

#[cfg(feature = "ssr")]
fn normalize_months(values: Vec<i32>) -> Vec<i32> {
    normalize_range(values, 1, 12)
}

#[cfg(feature = "ssr")]
fn normalize_range(mut values: Vec<i32>, min: i32, max: i32) -> Vec<i32> {
    values.retain(|value| (*value >= min) && (*value <= max));
    values.sort_unstable();
    values.dedup();
    if values.is_empty() {
        (min..=max).collect()
    } else {
        values
    }
}

#[cfg(feature = "ssr")]
fn parse_create_kind(kind: &str) -> Result<i32, ServerFnError> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "" | "classic" => Ok(KIND_CLASSIC),
        "block" => Ok(KIND_BLOCK),
        "sequential" => Ok(KIND_SEQUENTIAL),
        "scripted" => Ok(KIND_SCRIPTED),
        "externaljson" | "external-json" | "external_json" => Ok(KIND_EXTERNAL_JSON),
        _ => Err(ServerFnError::new("Unsupported playout kind")),
    }
}

#[cfg(feature = "ssr")]
fn schedule_file_missing_message(kind: i32) -> &'static str {
    match kind {
        KIND_EXTERNAL_JSON => "External Json File does not exist!",
        KIND_SEQUENTIAL => "Sequential schedule does not exist!",
        KIND_SCRIPTED => "Scripted schedule does not exist!",
        _ => "Schedule file does not exist",
    }
}

#[cfg(feature = "ssr")]
fn validate_scripted_schedule_file(schedule_file: &str) -> Result<(), ServerFnError> {
    let script_file = first_command_token(schedule_file);
    if script_file.is_empty() || std::fs::metadata(&script_file).is_err() {
        return Err(ServerFnError::new(schedule_file_missing_message(
            KIND_SCRIPTED,
        )));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
fn first_command_token(value: &str) -> String {
    let trimmed = value.trim_start();
    let Some(first) = trimmed.chars().next() else {
        return String::new();
    };

    if first != '"' && first != '\'' {
        return trimmed
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
    }

    // Mirror .NET's `CommandLineParser.SplitCommandLine`, which ErsatzTV uses:
    // backslashes inside a quoted token are literal, not escape characters, so
    // a Windows path like `"C:\Program Files\schedule.lua"` keeps its
    // separators. Reading until the matching close quote is sufficient.
    let quote = first;
    let mut token = String::new();
    for ch in trimmed[quote.len_utf8()..].chars() {
        if ch == quote {
            return token;
        }
        token.push(ch);
    }
    token
}

/// Now, formatted to match the stored UTC `Start`/`Finish` text columns for a
/// lexicographic `Finish >= now` comparison.
#[cfg(feature = "ssr")]
fn now_sql() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Parses a stored UTC datetime string and formats it in local time
/// (`StartOffset`/`FinishOffset` in ErsatzTV).
#[cfg(feature = "ssr")]
fn format_local(value: &str) -> String {
    use chrono::{Local, TimeZone};
    match parse_utc(value) {
        Some(dt) => Local
            .from_utc_datetime(&dt.naive_utc())
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => value.to_owned(),
    }
}

/// `PlayoutItem.GetDisplayDuration` parity: `mm:ss`, `h:mm:ss` (>= 1h), or
/// `H:mm:ss` (>= 24h, hours not zero-padded).
#[cfg(feature = "ssr")]
fn display_duration(start: &str, finish: &str) -> String {
    let (Some(s), Some(f)) = (parse_utc(start), parse_utc(finish)) else {
        return String::new();
    };
    let total = (f - s).num_seconds().max(0);
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    // ErsatzTV: `mm:ss` under an hour, otherwise `h:mm:ss` (hours not
    // zero-padded, so 24h+ keeps growing the leading field).
    if hours >= 1 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

#[cfg(feature = "ssr")]
fn parse_utc(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{NaiveDateTime, TimeZone, Utc};
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value.trim(), fmt) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    None
}

/// `HH:MM:SS` → minutes since midnight.
#[cfg(feature = "ssr")]
fn parse_daily_rebuild_minutes(value: &str) -> Option<i32> {
    let trimmed = value.trim();
    let mut parts = trimmed.split(':');
    let hours: i32 = parts.next()?.parse().ok()?;
    let minutes: i32 = parts.next().unwrap_or("0").parse().ok()?;
    Some(hours * 60 + minutes)
}

/// minutes since midnight → `HH:MM:SS`.
#[cfg(feature = "ssr")]
fn format_daily_rebuild_time(minutes: i32) -> String {
    let minutes = minutes.clamp(0, 23 * 60 + 59);
    format!("{:02}:{:02}:00", minutes / 60, minutes % 60)
}

/// Resolves ErsatzTV `GetDisplayTitle` for the given item ids in one query,
/// building the title in Rust (so no `printf`/`LPAD` SQL backend split).
#[cfg(feature = "ssr")]
async fn load_item_titles(
    pool: &ersatzrs_infra::db::DbPool,
    item_ids: &[i64],
) -> Result<std::collections::HashMap<i64, String>, ServerFnError> {
    use sqlx::Row;
    use std::collections::HashMap;

    let mut titles = HashMap::new();
    if item_ids.is_empty() {
        return Ok(titles);
    }

    let placeholders = vec!["?"; item_ids.len()].join(", ");

    let sql = format!(
        r#"SELECT pi."Id" AS "Id",
                  pi."ChapterTitle" AS "ChapterTitle",
                  EXISTS(SELECT 1 FROM "Episode" e WHERE e."Id" = pi."MediaItemId") AS "IsEpisode",
                  EXISTS(SELECT 1 FROM "Movie" m WHERE m."Id" = pi."MediaItemId") AS "IsMovie",
                  EXISTS(SELECT 1 FROM "MusicVideo" mv WHERE mv."Id" = pi."MediaItemId") AS "IsMusicVideo",
                  EXISTS(SELECT 1 FROM "OtherVideo" ov WHERE ov."Id" = pi."MediaItemId") AS "IsOtherVideo",
                  EXISTS(SELECT 1 FROM "Song" s WHERE s."Id" = pi."MediaItemId") AS "IsSong",
                  EXISTS(SELECT 1 FROM "Image" i WHERE i."Id" = pi."MediaItemId") AS "IsImage",
                  EXISTS(SELECT 1 FROM "RemoteStream" rs WHERE rs."Id" = pi."MediaItemId") AS "IsRemoteStream",
                  (SELECT sm."Title" FROM "Episode" e
                     JOIN "Season" se ON se."Id" = e."SeasonId"
                     JOIN "ShowMetadata" sm ON sm."ShowId" = se."ShowId"
                   WHERE e."Id" = pi."MediaItemId" LIMIT 1) AS "ShowTitle",
                  (SELECT se."SeasonNumber" FROM "Episode" e
                     JOIN "Season" se ON se."Id" = e."SeasonId"
                   WHERE e."Id" = pi."MediaItemId" LIMIT 1) AS "SeasonNumber",
                  (SELECT em."EpisodeNumber" FROM "EpisodeMetadata" em
                   WHERE em."EpisodeId" = pi."MediaItemId" LIMIT 1) AS "EpisodeNumber",
                  (SELECT em."Title" FROM "EpisodeMetadata" em
                   WHERE em."EpisodeId" = pi."MediaItemId" LIMIT 1) AS "EpisodeTitle",
                  (SELECT mm."Title" FROM "MovieMetadata" mm
                   WHERE mm."MovieId" = pi."MediaItemId" LIMIT 1) AS "MovieTitle",
                  (SELECT am."Title" FROM "MusicVideo" mv
                     JOIN "ArtistMetadata" am ON am."ArtistId" = mv."ArtistId"
                   WHERE mv."Id" = pi."MediaItemId" LIMIT 1) AS "MvArtist",
                  (SELECT mvm."Title" FROM "MusicVideoMetadata" mvm
                   WHERE mvm."MusicVideoId" = pi."MediaItemId" LIMIT 1) AS "MvTitle",
                  (SELECT smd."Title" FROM "SongMetadata" smd
                   WHERE smd."SongId" = pi."MediaItemId" LIMIT 1) AS "SongTitle",
                  (SELECT smd."Artists" FROM "SongMetadata" smd
                   WHERE smd."SongId" = pi."MediaItemId" LIMIT 1) AS "SongArtists",
                  (SELECT ovm."Title" FROM "OtherVideoMetadata" ovm
                   WHERE ovm."OtherVideoId" = pi."MediaItemId" LIMIT 1) AS "OtherTitle",
                  (SELECT im."Title" FROM "ImageMetadata" im
                   WHERE im."ImageId" = pi."MediaItemId" LIMIT 1) AS "ImageTitle",
                  (SELECT rsm."Title" FROM "RemoteStreamMetadata" rsm
                   WHERE rsm."RemoteStreamId" = pi."MediaItemId" LIMIT 1) AS "RemoteTitle"
           FROM "PlayoutItem" pi
           WHERE pi."Id" IN ({placeholders})"#
    );

    let mut q = sqlx::query(&sql);
    for id in item_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await?;

    for r in &rows {
        let id: i64 = r.try_get("Id").unwrap_or(0);
        let chapter: Option<String> = r
            .try_get::<Option<String>, _>("ChapterTitle")
            .ok()
            .flatten()
            .filter(|v| !v.trim().is_empty());

        let is_episode = r.try_get::<i64, _>("IsEpisode").unwrap_or(0) != 0;
        let is_movie = r.try_get::<i64, _>("IsMovie").unwrap_or(0) != 0;
        let is_music_video = r.try_get::<i64, _>("IsMusicVideo").unwrap_or(0) != 0;
        let is_other_video = r.try_get::<i64, _>("IsOtherVideo").unwrap_or(0) != 0;
        let is_song = r.try_get::<i64, _>("IsSong").unwrap_or(0) != 0;
        let is_image = r.try_get::<i64, _>("IsImage").unwrap_or(0) != 0;
        let is_remote_stream = r.try_get::<i64, _>("IsRemoteStream").unwrap_or(0) != 0;
        let show_title: Option<String> = r.try_get("ShowTitle").ok().flatten();
        let season_number: Option<i64> = r.try_get("SeasonNumber").ok().flatten();
        let episode_number: Option<i64> = r.try_get("EpisodeNumber").ok().flatten();
        let episode_title: Option<String> = r.try_get("EpisodeTitle").ok().flatten();
        let movie_title: Option<String> = r.try_get("MovieTitle").ok().flatten();
        let mv_artist: Option<String> = r.try_get("MvArtist").ok().flatten();
        let mv_title: Option<String> = r.try_get("MvTitle").ok().flatten();
        let song_title: Option<String> = r.try_get("SongTitle").ok().flatten();
        let song_artists: Option<String> = r.try_get("SongArtists").ok().flatten();
        let other_title: Option<String> = r.try_get("OtherTitle").ok().flatten();
        let image_title: Option<String> = r.try_get("ImageTitle").ok().flatten();
        let remote_title: Option<String> = r.try_get("RemoteTitle").ok().flatten();

        let title = if is_episode {
            if episode_number.is_none() || episode_title.is_none() {
                "[unknown episode]".to_owned()
            } else {
                let show = show_title
                    .as_deref()
                    .map(|s| format!("{s} - "))
                    .unwrap_or_default();
                let season = season_number.unwrap_or(0);
                let episode = episode_number.unwrap_or(0);
                let ep_title = episode_title.unwrap_or_default();
                with_chapter(
                    format!("{show}s{season:02}e{episode:02} - {ep_title}"),
                    chapter.as_deref(),
                )
            }
        } else if is_movie {
            movie_title.unwrap_or_else(|| "[unknown movie]".to_owned())
        } else if is_music_video {
            let Some(mv_title) = mv_title else {
                titles.insert(id, "[unknown music video]".to_owned());
                continue;
            };
            let artist = mv_artist
                .as_deref()
                .map(|a| format!("{a} - "))
                .unwrap_or_default();
            with_chapter(format!("{artist}{mv_title}"), chapter.as_deref())
        } else if is_other_video {
            with_chapter(
                other_title.unwrap_or_else(|| "[unknown video]".to_owned()),
                chapter.as_deref(),
            )
        } else if is_song {
            let Some(song_title) = song_title else {
                titles.insert(id, "[unknown song]".to_owned());
                continue;
            };
            let artists = song_artists
                .as_deref()
                .filter(|a| !a.trim().is_empty())
                .map(|a| format!("{} - ", normalize_song_artists(a)))
                .unwrap_or_default();
            with_chapter(format!("{artists}{song_title}"), chapter.as_deref())
        } else if is_image {
            image_title.unwrap_or_else(|| "[unknown image]".to_owned())
        } else if is_remote_stream {
            remote_title.unwrap_or_else(|| "[unknown remote stream]".to_owned())
        } else {
            String::new()
        };

        titles.insert(id, title);
    }

    Ok(titles)
}

#[cfg(feature = "ssr")]
fn normalize_song_artists(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<String>>(trimmed)
            .map_or_else(|_| trimmed.to_owned(), |artists| artists.join(", "))
    } else {
        trimmed.to_owned()
    }
}

#[cfg(feature = "ssr")]
fn with_chapter(title: String, chapter: Option<&str>) -> String {
    match chapter {
        Some(c) if !c.trim().is_empty() => format!("{title} ({c})"),
        _ => title,
    }
}

/// Enriches a Classic/Block scheduling-context JSON blob with human-readable
/// names. Ports `ProcessSchedulingContextHandler`.
#[cfg(feature = "ssr")]
async fn enrich_scheduling_context(pool: &ersatzrs_infra::db::DbPool, serialized: &str) -> String {
    use serde_json::Value;

    let Ok(value) = serde_json::from_str::<Value>(serialized) else {
        return serialized.to_owned();
    };
    let Some(obj) = value.as_object() else {
        return pretty(&value, serialized);
    };

    if let Some(schedule_id) = obj.get("ScheduleId").and_then(Value::as_i64) {
        if schedule_id > 0 {
            return enrich_classic(pool, obj, schedule_id).await;
        }
    } else if let Some(block_id) = obj.get("BlockId").and_then(Value::as_i64) {
        if block_id > 0 {
            return enrich_block(pool, obj, block_id).await;
        }
    }

    pretty(&value, serialized)
}

#[cfg(feature = "ssr")]
fn pretty(value: &serde_json::Value, original: &str) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| original.to_owned())
}

#[cfg(feature = "ssr")]
async fn enrich_classic(
    pool: &ersatzrs_infra::db::DbPool,
    obj: &serde_json::Map<String, serde_json::Value>,
    schedule_id: i64,
) -> String {
    use serde_json::{json, Value};

    let item_id = obj.get("ItemId").and_then(Value::as_i64).unwrap_or(0);
    let filler_preset_id = obj.get("FillerPresetId").and_then(Value::as_i64);

    let schedule_name: Option<String> =
        sqlx::query_scalar(r#"SELECT "Name" FROM "ProgramSchedule" WHERE "Id" = ?"#)
            .bind(schedule_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    let schedule_item = collection_ref_for_schedule_item(pool, item_id).await;

    let mut name = "Classic";
    let mut filler_value = Value::Null;
    if let Some(preset_id) = filler_preset_id {
        if preset_id > 0 {
            name = "Classic - Filler";
            filler_value = filler_preset_value(pool, preset_id).await;
        }
    }

    let context = json!({
        "Scheduler": { "Type": name, "Mode": obj.get("Scheduler") },
        "Schedule": { "Id": schedule_id, "Name": schedule_name.unwrap_or_default() },
        "ScheduleItem": schedule_item.into_value(item_id),
        "Filler": filler_value,
        "Enumerator": enumerator_value(obj),
    });
    serde_json::to_string_pretty(&context).unwrap_or_default()
}

#[cfg(feature = "ssr")]
async fn enrich_block(
    pool: &ersatzrs_infra::db::DbPool,
    obj: &serde_json::Map<String, serde_json::Value>,
    block_id: i64,
) -> String {
    use serde_json::{json, Value};
    use sqlx::Row;

    let block_item_id = obj.get("BlockItemId").and_then(Value::as_i64).unwrap_or(0);

    let block = sqlx::query(
        r#"SELECT b."Name" AS "Name", bg."Name" AS "Group"
           FROM "Block" b JOIN "BlockGroup" bg ON bg."Id" = b."BlockGroupId"
           WHERE b."Id" = ?"#,
    )
    .bind(block_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (group, block_name) = block
        .map(|r| {
            (
                r.try_get::<Option<String>, _>("Group")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                r.try_get::<Option<String>, _>("Name")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
            )
        })
        .unwrap_or_default();

    let block_item = collection_ref_for_block_item(pool, block_item_id).await;

    let context = json!({
        "Scheduler": { "Type": "Block", "Mode": Value::Null },
        "Block": { "Id": block_id, "Group": group, "Name": block_name },
        "BlockItem": block_item.into_value(block_item_id),
        "Enumerator": enumerator_value(obj),
    });
    serde_json::to_string_pretty(&context).unwrap_or_default()
}

#[cfg(feature = "ssr")]
fn enumerator_value(obj: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    use serde_json::{json, Value};
    json!({
        "Name": obj.get("Enumerator").cloned().unwrap_or(Value::Null),
        "Seed": obj.get("Seed").cloned().unwrap_or(Value::Null),
        "Index": obj.get("Index").cloned().unwrap_or(Value::Null),
    })
}

/// (collection-type integer, resolved collection name) for a schedule/block item.
#[cfg(feature = "ssr")]
struct CollectionRef {
    collection_type: Option<i32>,
    collection_name: Option<String>,
}

#[cfg(feature = "ssr")]
impl CollectionRef {
    fn into_value(self, id: i64) -> serde_json::Value {
        use serde_json::json;
        json!({
            "Id": id,
            "CollectionType": self.collection_type.map(collection_type_name),
            "CollectionName": self.collection_name,
        })
    }
}

#[cfg(feature = "ssr")]
async fn collection_ref_for_schedule_item(
    pool: &ersatzrs_infra::db::DbPool,
    item_id: i64,
) -> CollectionRef {
    use sqlx::Row;
    let Some(row) = sqlx::query(
        r#"SELECT "CollectionType", "CollectionId", "MultiCollectionId", "PlaylistId",
                  "RerunCollectionId", "SmartCollectionId"
           FROM "ProgramScheduleItem" WHERE "Id" = ?"#,
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten() else {
        return CollectionRef {
            collection_type: None,
            collection_name: None,
        };
    };
    let ct: i32 = row.try_get("CollectionType").unwrap_or(0);
    let name = resolve_collection_name(
        pool,
        ct,
        row.try_get("CollectionId").ok().flatten(),
        row.try_get("MultiCollectionId").ok().flatten(),
        row.try_get("PlaylistId").ok().flatten(),
        row.try_get("RerunCollectionId").ok().flatten(),
        row.try_get("SmartCollectionId").ok().flatten(),
    )
    .await;
    CollectionRef {
        collection_type: Some(ct),
        collection_name: name,
    }
}

#[cfg(feature = "ssr")]
async fn collection_ref_for_block_item(
    pool: &ersatzrs_infra::db::DbPool,
    item_id: i64,
) -> CollectionRef {
    use sqlx::Row;
    let Some(row) = sqlx::query(
        r#"SELECT "CollectionType", "CollectionId", "MultiCollectionId", "SmartCollectionId"
           FROM "BlockItem" WHERE "Id" = ?"#,
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten() else {
        return CollectionRef {
            collection_type: None,
            collection_name: None,
        };
    };
    let ct: i32 = row.try_get("CollectionType").unwrap_or(0);
    let name = resolve_collection_name(
        pool,
        ct,
        row.try_get("CollectionId").ok().flatten(),
        row.try_get("MultiCollectionId").ok().flatten(),
        None,
        None,
        row.try_get("SmartCollectionId").ok().flatten(),
    )
    .await;
    CollectionRef {
        collection_type: Some(ct),
        collection_name: name,
    }
}

#[cfg(feature = "ssr")]
async fn filler_preset_value(
    pool: &ersatzrs_infra::db::DbPool,
    preset_id: i64,
) -> serde_json::Value {
    use serde_json::{json, Value};
    use sqlx::Row;
    let Some(row) = sqlx::query(
        r#"SELECT "Name", "FillerKind", "FillerMode", "CollectionType",
                  "CollectionId", "MultiCollectionId", "PlaylistId", "SmartCollectionId"
           FROM "FillerPreset" WHERE "Id" = ?"#,
    )
    .bind(preset_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten() else {
        return Value::Null;
    };
    let ct: i32 = row.try_get("CollectionType").unwrap_or(0);
    let name = resolve_collection_name(
        pool,
        ct,
        row.try_get("CollectionId").ok().flatten(),
        row.try_get("MultiCollectionId").ok().flatten(),
        row.try_get("PlaylistId").ok().flatten(),
        None,
        row.try_get("SmartCollectionId").ok().flatten(),
    )
    .await;
    json!({
        "Id": preset_id,
        "Name": row.try_get::<Option<String>, _>("Name").ok().flatten(),
        "Kind": filler_kind_name(row.try_get("FillerKind").unwrap_or(0)),
        "Mode": filler_mode_name(row.try_get("FillerMode").unwrap_or(0)),
        "CollectionType": collection_type_name(ct),
        "CollectionName": name,
    })
}

#[cfg(feature = "ssr")]
#[allow(clippy::too_many_arguments)]
async fn resolve_collection_name(
    pool: &ersatzrs_infra::db::DbPool,
    collection_type: i32,
    collection_id: Option<i64>,
    multi_collection_id: Option<i64>,
    playlist_id: Option<i64>,
    rerun_collection_id: Option<i64>,
    smart_collection_id: Option<i64>,
) -> Option<String> {
    // CollectionType: 0 Collection, 4 MultiCollection, 5 SmartCollection,
    // 6 Playlist, 7 RerunFirstRun, 8 RerunRerun.
    let (table, id) = match collection_type {
        0 => ("Collection", collection_id),
        4 => ("MultiCollection", multi_collection_id),
        5 => ("SmartCollection", smart_collection_id),
        6 => ("Playlist", playlist_id),
        7 | 8 => ("RerunCollection", rerun_collection_id),
        _ => return None,
    };
    let id = id?;
    sqlx::query_scalar(&format!(r#"SELECT "Name" FROM "{table}" WHERE "Id" = ?"#))
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

#[cfg(feature = "ssr")]
fn collection_type_name(value: i32) -> &'static str {
    match value {
        0 => "Collection",
        1 => "TelevisionShow",
        2 => "TelevisionSeason",
        3 => "Artist",
        4 => "MultiCollection",
        5 => "SmartCollection",
        6 => "Playlist",
        7 => "RerunFirstRun",
        8 => "RerunRerun",
        9 => "SearchQuery",
        10 => "Movie",
        20 => "Episode",
        _ => "Unknown",
    }
}

#[cfg(feature = "ssr")]
fn filler_kind_name(value: i32) -> &'static str {
    match value {
        0 => "None",
        1 => "PreRoll",
        2 => "MidRoll",
        3 => "PostRoll",
        4 => "Tail",
        5 => "Fallback",
        99 => "GuideMode",
        100 => "DecoDefault",
        _ => "Unknown",
    }
}

#[cfg(feature = "ssr")]
fn filler_mode_name(value: i32) -> &'static str {
    match value {
        0 => "None",
        1 => "Duration",
        2 => "Count",
        3 => "Pad",
        4 => "RandomCount",
        _ => "Unknown",
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    async fn test_db() -> ersatzrs_infra::db::DbPool {
        #[cfg(feature = "mysql")]
        {
            sqlx::any::install_default_drivers();
            sqlx::any::AnyPoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("pool")
        }

        #[cfg(not(feature = "mysql"))]
        {
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("pool")
        }
    }

    #[test]
    fn display_duration_uses_minute_seconds_under_an_hour() {
        // 00:05:30 -> 00:10:00 == 4m30s
        assert_eq!(
            display_duration("2024-01-01 00:05:30", "2024-01-01 00:10:00"),
            "04:30"
        );
    }

    #[test]
    fn display_duration_uses_hours_at_or_above_one_hour() {
        assert_eq!(
            display_duration("2024-01-01 00:00:00", "2024-01-01 02:03:04"),
            "2:03:04"
        );
        // 25h keeps growing the leading hours field (ErsatzTV behavior).
        assert_eq!(
            display_duration("2024-01-01 00:00:00", "2024-01-02 01:00:00"),
            "25:00:00"
        );
    }

    #[test]
    fn daily_rebuild_time_round_trips() {
        assert_eq!(parse_daily_rebuild_minutes("06:30:00"), Some(390));
        assert_eq!(parse_daily_rebuild_minutes("00:30"), Some(30));
        assert_eq!(format_daily_rebuild_time(390), "06:30:00");
        assert_eq!(format_daily_rebuild_time(30), "00:30:00");
        assert_eq!(format_daily_rebuild_time(23 * 60 + 30), "23:30:00");
    }

    #[test]
    fn with_chapter_appends_when_present() {
        assert_eq!(with_chapter("Movie".into(), None), "Movie");
        assert_eq!(with_chapter("Movie".into(), Some("  ")), "Movie");
        assert_eq!(with_chapter("Movie".into(), Some("Ch 1")), "Movie (Ch 1)");
    }

    #[test]
    fn song_artists_json_is_displayed_like_ersatztv_list_join() {
        assert_eq!(normalize_song_artists(r#"["One","Two"]"#), "One, Two");
        assert_eq!(normalize_song_artists("One, Two"), "One, Two");
    }

    #[test]
    fn scripted_schedule_validation_uses_first_command_token() {
        assert_eq!(
            first_command_token("/tmp/script.lua --flag value"),
            "/tmp/script.lua"
        );
        assert_eq!(
            first_command_token(r#""/tmp/script file.lua" --flag value"#),
            "/tmp/script file.lua"
        );
        assert_eq!(
            first_command_token(r"'/tmp/script file.lua' --flag value"),
            "/tmp/script file.lua"
        );
        // Backslashes inside a quoted token are literal path separators, not
        // escapes (parity with .NET `SplitCommandLine`); a Windows path must
        // survive intact so `std::fs::metadata` can find it.
        assert_eq!(
            first_command_token(r#""C:\Program Files\schedule.lua" --slot prime"#),
            r"C:\Program Files\schedule.lua"
        );
        assert_eq!(
            first_command_token(r"C:\scripts\schedule.lua --slot prime"),
            r"C:\scripts\schedule.lua"
        );
    }

    #[test]
    fn playout_seed_is_non_negative_and_varies() {
        // Mirrors ErsatzTV's `Random().Next()` range (`0..=i32::MAX`).
        assert!(next_seed() >= 0);
        // Distinct calls should diverge; collisions are astronomically unlikely.
        let seeds: std::collections::HashSet<i32> = (0..16).map(|_| next_seed()).collect();
        assert!(seeds.len() > 1, "next_seed produced no variety");
    }

    #[tokio::test]
    async fn display_title_keeps_movie_chapter_out_and_uses_unknown_fallback() {
        let pool = test_db().await;
        create_title_projection_tables(&pool).await;

        sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (10), (11)"#)
            .execute(&pool)
            .await
            .expect("movies");
        sqlx::query(r#"INSERT INTO "MovieMetadata" ("MovieId", "Title") VALUES (10, 'Feature')"#)
            .execute(&pool)
            .await
            .expect("movie metadata");
        sqlx::query(
            r#"INSERT INTO "PlayoutItem" ("Id", "MediaItemId", "ChapterTitle") VALUES
               (1, 10, 'Chapter One'),
               (2, 11, NULL)"#,
        )
        .execute(&pool)
        .await
        .expect("playout items");

        let titles = load_item_titles(&pool, &[1, 2]).await.expect("titles");
        assert_eq!(titles.get(&1).map(String::as_str), Some("Feature"));
        assert_eq!(titles.get(&2).map(String::as_str), Some("[unknown movie]"));
    }

    #[tokio::test]
    async fn display_title_includes_song_artists_and_chapter() {
        let pool = test_db().await;
        create_title_projection_tables(&pool).await;

        sqlx::query(r#"INSERT INTO "Song" ("Id") VALUES (20)"#)
            .execute(&pool)
            .await
            .expect("song");
        sqlx::query(
            r#"INSERT INTO "SongMetadata" ("SongId", "Title", "Artists")
               VALUES (20, 'Track', '["One","Two"]')"#,
        )
        .execute(&pool)
        .await
        .expect("song metadata");
        sqlx::query(
            r#"INSERT INTO "PlayoutItem" ("Id", "MediaItemId", "ChapterTitle")
               VALUES (3, 20, 'Live')"#,
        )
        .execute(&pool)
        .await
        .expect("playout item");

        let titles = load_item_titles(&pool, &[3]).await.expect("titles");
        assert_eq!(
            titles.get(&3).map(String::as_str),
            Some("One, Two - Track (Live)")
        );
    }

    #[test]
    fn enum_name_helpers_match_ersatztv() {
        assert_eq!(collection_type_name(4), "MultiCollection");
        assert_eq!(collection_type_name(8), "RerunRerun");
        assert_eq!(filler_kind_name(2), "MidRoll");
        assert_eq!(filler_mode_name(3), "Pad");
    }

    #[tokio::test]
    async fn scheduling_context_pretty_prints_unknown_json() {
        let pool = test_db().await;
        // No ScheduleId/BlockId -> pretty-printed without touching the DB.
        let out = enrich_scheduling_context(&pool, r#"{"Foo":1}"#).await;
        assert!(
            out.contains("\"Foo\": 1"),
            "expected pretty JSON, got: {out}"
        );
    }

    #[tokio::test]
    async fn scheduling_context_returns_raw_for_invalid_json() {
        let pool = test_db().await;
        let out = enrich_scheduling_context(&pool, "not json").await;
        assert_eq!(out, "not json");
    }

    #[tokio::test]
    async fn create_classic_playout_requires_schedule_items_and_inserts() {
        let pool = test_db().await;
        create_playout_create_tables(&pool).await;
        seed_create_channel(&pool, 10).await;
        sqlx::query(r#"INSERT INTO "ProgramSchedule" ("Id", "Name") VALUES (20, 'Prime')"#)
            .execute(&pool)
            .await
            .expect("schedule");

        let empty = create_playout_with_pool(
            &pool,
            &PlayoutCreateForm {
                kind: "Classic".to_owned(),
                channel_id: 10,
                program_schedule_id: Some(20),
                schedule_file: String::new(),
            },
        )
        .await
        .expect_err("empty schedule rejected");
        assert!(empty
            .to_string()
            .contains("Program schedule must have items"));

        sqlx::query(
            r#"INSERT INTO "ProgramScheduleItem" ("Id", "ProgramScheduleId") VALUES (30, 20)"#,
        )
        .execute(&pool)
        .await
        .expect("schedule item");

        let id = create_playout_with_pool(
            &pool,
            &PlayoutCreateForm {
                kind: "Classic".to_owned(),
                channel_id: 10,
                program_schedule_id: Some(20),
                schedule_file: String::new(),
            },
        )
        .await
        .expect("created");

        let row: (i64, i32, Option<i64>, Option<String>) = sqlx::query_as(
            r#"SELECT "ChannelId", "ScheduleKind", "ProgramScheduleId", "ScheduleFile"
               FROM "Playout" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("playout");
        assert_eq!(row, (10, KIND_CLASSIC, Some(20), None));
    }

    #[tokio::test]
    async fn create_playout_rejects_channel_that_already_has_playout() {
        let pool = test_db().await;
        create_playout_create_tables(&pool).await;
        seed_create_channel(&pool, 11).await;
        sqlx::query(
            r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
               VALUES (50, 11, 2, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("existing playout");

        let error = create_playout_with_pool(
            &pool,
            &PlayoutCreateForm {
                kind: "Block".to_owned(),
                channel_id: 11,
                ..PlayoutCreateForm::default()
            },
        )
        .await
        .expect_err("duplicate rejected");

        assert!(error
            .to_string()
            .contains("Channel already has one playout"));
    }

    #[tokio::test]
    async fn create_sequential_playout_requires_existing_file() {
        let pool = test_db().await;
        create_playout_create_tables(&pool).await;
        seed_create_channel(&pool, 12).await;

        let missing = create_playout_with_pool(
            &pool,
            &PlayoutCreateForm {
                kind: "Sequential".to_owned(),
                channel_id: 12,
                schedule_file: "/definitely/missing/schedule.yml".to_owned(),
                ..PlayoutCreateForm::default()
            },
        )
        .await
        .expect_err("missing file rejected");
        assert!(missing
            .to_string()
            .contains("Sequential schedule does not exist!"));

        let file = tempfile::NamedTempFile::new().expect("temp file");
        let path = file.path().to_string_lossy().into_owned();
        let id = create_playout_with_pool(
            &pool,
            &PlayoutCreateForm {
                kind: "Sequential".to_owned(),
                channel_id: 12,
                schedule_file: path.clone(),
                ..PlayoutCreateForm::default()
            },
        )
        .await
        .expect("created");

        let row: (i32, String) = sqlx::query_as(
            r#"SELECT "ScheduleKind", "ScheduleFile" FROM "Playout" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("playout");
        assert_eq!(row, (KIND_SEQUENTIAL, path));
    }

    #[tokio::test]
    async fn create_scripted_playout_accepts_script_path_with_arguments() {
        let pool = test_db().await;
        create_playout_create_tables(&pool).await;
        seed_create_channel(&pool, 13).await;

        let file = tempfile::NamedTempFile::new().expect("temp file");
        let path = file.path().to_string_lossy().into_owned();
        let command = format!("\"{path}\" --slot prime");
        let id = create_playout_with_pool(
            &pool,
            &PlayoutCreateForm {
                kind: "Scripted".to_owned(),
                channel_id: 13,
                schedule_file: command.clone(),
                ..PlayoutCreateForm::default()
            },
        )
        .await
        .expect("created");

        let row: (i32, String) = sqlx::query_as(
            r#"SELECT "ScheduleKind", "ScheduleFile" FROM "Playout" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("playout");
        assert_eq!(row, (KIND_SCRIPTED, command));
    }

    #[tokio::test]
    async fn alternate_schedules_append_default_and_save_default_from_last_row() {
        let pool = test_db().await;
        create_nested_playout_tables(&pool).await;
        seed_nested_channel_and_playouts(&pool).await;
        sqlx::query(
            r#"INSERT INTO "ProgramSchedule" ("Id", "Name")
               VALUES (10, 'Default'), (11, 'Weekends'), (12, 'Late')"#,
        )
        .execute(&pool)
        .await
        .expect("schedules");
        sqlx::query(
            r#"INSERT INTO "ProgramScheduleAlternate"
               ("Id", "PlayoutId", "ProgramScheduleId", "Index", "DaysOfWeek",
                "DaysOfMonth", "MonthsOfYear", "LimitToDateRange", "StartMonth",
                "StartDay", "EndMonth", "EndDay")
               VALUES (20, 100, 11, 1, '["Saturday","Sunday"]', '1,2', '1',
                       0, 1, 1, 12, 31)"#,
        )
        .execute(&pool)
        .await
        .expect("alternate");

        let form = get_playout_alternate_schedules_with_pool(&pool, 100)
            .await
            .expect("form")
            .expect("classic playout");
        assert_eq!(form.items.len(), 2);
        assert_eq!(form.items[0].date_filter.days_of_week, vec![0, 6]);
        assert_eq!(form.items[1].id, -1);
        assert_eq!(form.items[1].program_schedule_id, 10);

        let mut saved = form.items;
        saved[0].program_schedule_id = 12;
        saved[1].program_schedule_id = 11;
        save_playout_alternate_schedules_with_pool(&pool, 100, &saved)
            .await
            .expect("saved");

        let default_id: Option<i64> =
            sqlx::query_scalar(r#"SELECT "ProgramScheduleId" FROM "Playout" WHERE "Id" = 100"#)
                .fetch_one(&pool)
                .await
                .expect("default");
        assert_eq!(default_id, Some(11));

        let rows: Vec<(i64, i64)> = sqlx::query_as(
            r#"SELECT "Id", "ProgramScheduleId"
               FROM "ProgramScheduleAlternate"
               WHERE "PlayoutId" = 100
               ORDER BY "Index""#,
        )
        .fetch_all(&pool)
        .await
        .expect("alternates");
        assert_eq!(rows, vec![(20, 12)]);
    }

    #[tokio::test]
    async fn playout_templates_save_replaces_rows() {
        let pool = test_db().await;
        create_nested_playout_tables(&pool).await;
        seed_nested_channel_and_playouts(&pool).await;
        sqlx::query(r#"INSERT INTO "TemplateGroup" ("Id", "Name") VALUES (1, 'Main')"#)
            .execute(&pool)
            .await
            .expect("template group");
        sqlx::query(
            r#"INSERT INTO "Template" ("Id", "TemplateGroupId", "Name", "DateUpdated")
               VALUES (10, 1, 'Prime', CURRENT_TIMESTAMP),
                      (11, 1, 'Late', CURRENT_TIMESTAMP)"#,
        )
        .execute(&pool)
        .await
        .expect("templates");
        sqlx::query(
            r#"INSERT INTO "PlayoutTemplate"
               ("Id", "PlayoutId", "TemplateId", "Index", "DaysOfWeek", "DaysOfMonth",
                "MonthsOfYear", "LimitToDateRange", "StartMonth", "StartDay",
                "EndMonth", "EndDay", "DateUpdated")
               VALUES (30, 101, 10, 1, '[1,2,3]', '[1]', '[1]', 0, 1, 1, 12, 31,
                       CURRENT_TIMESTAMP),
                      (31, 101, 11, 2, '[4]', '[2]', '[2]', 0, 1, 1, 12, 31,
                       CURRENT_TIMESTAMP)"#,
        )
        .execute(&pool)
        .await
        .expect("playout templates");

        let form = get_playout_templates_with_pool(&pool, 101)
            .await
            .expect("form")
            .expect("block playout");
        assert_eq!(form.items.len(), 2);

        let saved = vec![PlayoutTemplateItemForm {
            id: 30,
            index: 1,
            template_group_id: 1,
            template_id: 11,
            template_name: "Late".to_owned(),
            deco_template_group_id: None,
            deco_template_id: None,
            deco_template_name: None,
            date_filter: PlayoutDateFilter {
                days_of_week: vec![0, 6],
                days_of_month: vec![1, 15],
                months_of_year: vec![12],
                ..PlayoutDateFilter::default()
            },
        }];
        save_playout_templates_with_pool(&pool, 101, &saved)
            .await
            .expect("saved");

        let rows: Vec<(i64, i64, String)> = sqlx::query_as(
            r#"SELECT "Id", "TemplateId", "DaysOfWeek"
               FROM "PlayoutTemplate"
               WHERE "PlayoutId" = 101
               ORDER BY "Index""#,
        )
        .fetch_all(&pool)
        .await
        .expect("templates");
        assert_eq!(rows, vec![(30, 11, "[0,6]".to_owned())]);
    }

    async fn create_title_projection_tables(pool: &ersatzrs_infra::db::DbPool) {
        for sql in [
            r#"CREATE TABLE "PlayoutItem" ("Id" INTEGER, "MediaItemId" INTEGER, "ChapterTitle" TEXT)"#,
            r#"CREATE TABLE "Episode" ("Id" INTEGER, "SeasonId" INTEGER)"#,
            r#"CREATE TABLE "Movie" ("Id" INTEGER)"#,
            r#"CREATE TABLE "MusicVideo" ("Id" INTEGER, "ArtistId" INTEGER)"#,
            r#"CREATE TABLE "OtherVideo" ("Id" INTEGER)"#,
            r#"CREATE TABLE "Song" ("Id" INTEGER)"#,
            r#"CREATE TABLE "Image" ("Id" INTEGER)"#,
            r#"CREATE TABLE "RemoteStream" ("Id" INTEGER)"#,
            r#"CREATE TABLE "Season" ("Id" INTEGER, "ShowId" INTEGER, "SeasonNumber" INTEGER)"#,
            r#"CREATE TABLE "ShowMetadata" ("ShowId" INTEGER, "Title" TEXT)"#,
            r#"CREATE TABLE "EpisodeMetadata" ("EpisodeId" INTEGER, "EpisodeNumber" INTEGER, "Title" TEXT)"#,
            r#"CREATE TABLE "MovieMetadata" ("MovieId" INTEGER, "Title" TEXT)"#,
            r#"CREATE TABLE "ArtistMetadata" ("ArtistId" INTEGER, "Title" TEXT)"#,
            r#"CREATE TABLE "MusicVideoMetadata" ("MusicVideoId" INTEGER, "Title" TEXT)"#,
            r#"CREATE TABLE "SongMetadata" ("SongId" INTEGER, "Title" TEXT, "Artists" TEXT)"#,
            r#"CREATE TABLE "OtherVideoMetadata" ("OtherVideoId" INTEGER, "Title" TEXT)"#,
            r#"CREATE TABLE "ImageMetadata" ("ImageId" INTEGER, "Title" TEXT)"#,
            r#"CREATE TABLE "RemoteStreamMetadata" ("RemoteStreamId" INTEGER, "Title" TEXT)"#,
        ] {
            sqlx::query(sql).execute(pool).await.expect("create table");
        }
    }

    async fn create_playout_create_tables(pool: &ersatzrs_infra::db::DbPool) {
        for sql in [
            r#"CREATE TABLE "Channel" (
                "Id" INTEGER PRIMARY KEY,
                "Number" TEXT NULL,
                "Name" TEXT NULL,
                "SortNumber" REAL NOT NULL,
                "PlayoutMode" INTEGER NOT NULL
            )"#,
            r#"CREATE TABLE "Playout" (
                "Id" INTEGER PRIMARY KEY AUTOINCREMENT,
                "ChannelId" INTEGER NOT NULL,
                "ProgramScheduleId" INTEGER NULL,
                "ScheduleFile" TEXT NULL,
                "ScheduleKind" INTEGER NOT NULL,
                "Seed" INTEGER NOT NULL
            )"#,
            r#"CREATE TABLE "ProgramSchedule" (
                "Id" INTEGER PRIMARY KEY,
                "Name" TEXT NULL
            )"#,
            r#"CREATE TABLE "ProgramScheduleItem" (
                "Id" INTEGER PRIMARY KEY,
                "ProgramScheduleId" INTEGER NOT NULL
            )"#,
        ] {
            sqlx::query(sql).execute(pool).await.expect("create table");
        }
    }

    async fn seed_create_channel(pool: &ersatzrs_infra::db::DbPool, id: i64) {
        sqlx::query(
            r#"INSERT INTO "Channel" ("Id", "Number", "Name", "SortNumber", "PlayoutMode")
               VALUES (?, '1', 'One', 1.0, 0)"#,
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("channel");
    }

    async fn create_nested_playout_tables(pool: &ersatzrs_infra::db::DbPool) {
        for sql in [
            r#"CREATE TABLE "Channel" (
                "Id" INTEGER PRIMARY KEY,
                "Number" TEXT NULL,
                "Name" TEXT NULL,
                "SortNumber" REAL NOT NULL,
                "PlayoutMode" INTEGER NOT NULL
            )"#,
            r#"CREATE TABLE "Playout" (
                "Id" INTEGER PRIMARY KEY,
                "ChannelId" INTEGER NOT NULL,
                "ProgramScheduleId" INTEGER NULL,
                "ScheduleFile" TEXT NULL,
                "ScheduleKind" INTEGER NOT NULL,
                "Seed" INTEGER NOT NULL
            )"#,
            r#"CREATE TABLE "ProgramSchedule" (
                "Id" INTEGER PRIMARY KEY,
                "Name" TEXT NULL
            )"#,
            r#"CREATE TABLE "ProgramScheduleAlternate" (
                "Id" INTEGER PRIMARY KEY,
                "PlayoutId" INTEGER NOT NULL,
                "ProgramScheduleId" INTEGER NOT NULL,
                "Index" INTEGER NOT NULL,
                "DaysOfWeek" TEXT NULL,
                "DaysOfMonth" TEXT NULL,
                "MonthsOfYear" TEXT NULL,
                "LimitToDateRange" INTEGER NOT NULL DEFAULT 0,
                "StartMonth" INTEGER NOT NULL DEFAULT 1,
                "StartDay" INTEGER NOT NULL DEFAULT 1,
                "StartYear" INTEGER NULL,
                "EndMonth" INTEGER NOT NULL DEFAULT 12,
                "EndDay" INTEGER NOT NULL DEFAULT 31,
                "EndYear" INTEGER NULL
            )"#,
            r#"CREATE TABLE "TemplateGroup" (
                "Id" INTEGER PRIMARY KEY,
                "Name" TEXT NULL
            )"#,
            r#"CREATE TABLE "Template" (
                "Id" INTEGER PRIMARY KEY,
                "TemplateGroupId" INTEGER NOT NULL,
                "Name" TEXT NULL,
                "DateUpdated" TEXT NULL
            )"#,
            r#"CREATE TABLE "DecoTemplateGroup" (
                "Id" INTEGER PRIMARY KEY,
                "Name" TEXT NULL
            )"#,
            r#"CREATE TABLE "DecoTemplate" (
                "Id" INTEGER PRIMARY KEY,
                "DecoTemplateGroupId" INTEGER NOT NULL,
                "Name" TEXT NULL,
                "DateUpdated" TEXT NULL
            )"#,
            r#"CREATE TABLE "PlayoutTemplate" (
                "Id" INTEGER PRIMARY KEY,
                "DateUpdated" TEXT NULL,
                "DaysOfMonth" TEXT NULL,
                "DaysOfWeek" TEXT NULL,
                "DecoTemplateId" INTEGER NULL,
                "EndDay" INTEGER NOT NULL DEFAULT 31,
                "EndMonth" INTEGER NOT NULL DEFAULT 12,
                "Index" INTEGER NOT NULL,
                "LimitToDateRange" INTEGER NOT NULL DEFAULT 0,
                "MonthsOfYear" TEXT NULL,
                "PlayoutId" INTEGER NOT NULL,
                "StartDay" INTEGER NOT NULL DEFAULT 1,
                "StartMonth" INTEGER NOT NULL DEFAULT 1,
                "TemplateId" INTEGER NOT NULL,
                "EndYear" INTEGER NULL,
                "StartYear" INTEGER NULL
            )"#,
        ] {
            sqlx::query(sql).execute(pool).await.expect("create table");
        }
    }

    async fn seed_nested_channel_and_playouts(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(
            r#"INSERT INTO "Channel" ("Id", "Number", "Name", "SortNumber", "PlayoutMode")
               VALUES (1, '1', 'One', 1.0, 0)"#,
        )
        .execute(pool)
        .await
        .expect("channel");
        sqlx::query(
            r#"INSERT INTO "Playout"
               ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
               VALUES (100, 1, 10, 1, 1), (101, 1, NULL, 2, 2)"#,
        )
        .execute(pool)
        .await
        .expect("playouts");
    }
}
