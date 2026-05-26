//! Server functions for template group / template management and the template editor.
//! Ports:
//! - `Pages/Templates.razor` -> list, add, copy, and delete actions.
//! - `Pages/TemplateEditor.razor` -> template metadata, block add/remove, and item save.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use ersatzrs_infra::db::DbPool;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateGroupRow {
    pub id: i64,
    pub name: String,
    pub template_count: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateRow {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateForm {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateItemForm {
    pub block_id: i64,
    pub block_name: String,
    /// Minutes since midnight.
    pub start_minutes: i32,
    pub duration_minutes: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateWithItemsForm {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub items: Vec<TemplateItemForm>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateBlockGroupOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateBlockOption {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub minutes: i32,
}

#[server]
pub async fn list_template_groups() -> Result<Vec<TemplateGroupRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_template_groups_with_pool(&state.pool).await
}

#[cfg(feature = "ssr")]
pub async fn list_template_groups_with_pool(
    pool: &DbPool,
) -> Result<Vec<TemplateGroupRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT tg."Id",
                  COALESCE(tg."Name", '') AS "Name",
                  COUNT(t."Id") AS "TemplateCount"
           FROM "TemplateGroup" tg
           LEFT JOIN "Template" t ON t."TemplateGroupId" = tg."Id"
           GROUP BY tg."Id", tg."Name"
           ORDER BY tg."Name", tg."Id""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| TemplateGroupRow {
            id: row.try_get("Id").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
            template_count: row.try_get("TemplateCount").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn list_templates(search: String) -> Result<Vec<TemplateRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_templates_with_pool(&state.pool, &search).await
}

#[cfg(feature = "ssr")]
pub async fn list_templates_with_pool(
    pool: &DbPool,
    search: &str,
) -> Result<Vec<TemplateRow>, ServerFnError> {
    use sqlx::Row;

    let trimmed = search.trim();
    let like = format!("%{trimmed}%");
    let rows = sqlx::query(
        r#"SELECT t."Id",
                  t."TemplateGroupId",
                  COALESCE(tg."Name", '') AS "GroupName",
                  COALESCE(t."Name", '') AS "Name"
           FROM "Template" t
           JOIN "TemplateGroup" tg ON tg."Id" = t."TemplateGroupId"
           WHERE (? = '' OR t."Name" LIKE ?)
           UNION ALL
           SELECT -tg."Id" AS "Id",
                  tg."Id" AS "TemplateGroupId",
                  COALESCE(tg."Name", '') AS "GroupName",
                  '(none)' AS "Name"
           FROM "TemplateGroup" tg
           WHERE ? = ''
             AND NOT EXISTS (SELECT 1 FROM "Template" t WHERE t."TemplateGroupId" = tg."Id")
           ORDER BY "GroupName", "Name""#,
    )
    .bind(trimmed)
    .bind(&like)
    .bind(trimmed)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| TemplateRow {
            id: row.try_get("Id").unwrap_or_default(),
            group_id: row.try_get("TemplateGroupId").unwrap_or_default(),
            group_name: row.try_get("GroupName").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn create_template_group(name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    create_template_group_with_pool(&state.pool, &name).await
}

#[cfg(feature = "ssr")]
pub async fn create_template_group_with_pool(
    pool: &DbPool,
    name: &str,
) -> Result<i64, ServerFnError> {
    let name = validate_template_group_name(pool, name).await?;
    let result = sqlx::query(r#"INSERT INTO "TemplateGroup" ("Name") VALUES (?)"#)
        .bind(&name)
        .execute(pool)
        .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

#[server]
pub async fn create_template(group_id: i64, name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    create_template_with_pool(&state.pool, group_id, &name).await
}

#[cfg(feature = "ssr")]
pub async fn create_template_with_pool(
    pool: &DbPool,
    group_id: i64,
    name: &str,
) -> Result<i64, ServerFnError> {
    validate_template_group_exists(pool, group_id).await?;
    let name = validate_template_name(pool, group_id, None, name).await?;
    let result = sqlx::query(
        r#"INSERT INTO "Template" ("TemplateGroupId", "Name", "DateUpdated")
           VALUES (?, ?, CURRENT_TIMESTAMP)"#,
    )
    .bind(group_id)
    .bind(&name)
    .execute(pool)
    .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

#[server]
pub async fn copy_template(
    template_id: i64,
    new_group_id: i64,
    new_name: String,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    copy_template_with_pool(&state.pool, template_id, new_group_id, &new_name).await
}

#[cfg(feature = "ssr")]
pub async fn copy_template_with_pool(
    pool: &DbPool,
    template_id: i64,
    new_group_id: i64,
    new_name: &str,
) -> Result<i64, ServerFnError> {
    validate_template_group_exists(pool, new_group_id).await?;
    let new_name = validate_template_name(pool, new_group_id, None, new_name).await?;
    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Template" WHERE "Id" = ?"#)
        .bind(template_id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Template does not exist."));
    }

    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"INSERT INTO "Template" ("TemplateGroupId", "Name", "DateUpdated")
           VALUES (?, ?, CURRENT_TIMESTAMP)"#,
    )
    .bind(new_group_id)
    .bind(&new_name)
    .execute(&mut *tx)
    .await?;
    let new_template_id = ersatzrs_infra::db::last_insert_id(result);

    let items = sqlx::query(
        r#"SELECT "BlockId", "StartTime"
           FROM "TemplateItem"
           WHERE "TemplateId" = ?
           ORDER BY "StartTime", "Id""#,
    )
    .bind(template_id)
    .fetch_all(&mut *tx)
    .await?;

    for item in items {
        use sqlx::Row;
        sqlx::query(
            r#"INSERT INTO "TemplateItem" ("TemplateId", "BlockId", "StartTime")
               VALUES (?, ?, ?)"#,
        )
        .bind(new_template_id)
        .bind(item.try_get::<i64, _>("BlockId").unwrap_or_default())
        .bind(item.try_get::<String, _>("StartTime").unwrap_or_default())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(new_template_id)
}

#[server]
pub async fn delete_template_group(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    delete_template_group_with_pool(&state.pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn delete_template_group_with_pool(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let result = sqlx::query(r#"DELETE FROM "TemplateGroup" WHERE "Id" = ?"#)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerFnError::new(format!(
            "TemplateGroup {id} does not exist."
        )));
    }
    Ok(())
}

#[server]
pub async fn delete_template(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    delete_template_with_pool(&state.pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn delete_template_with_pool(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let result = sqlx::query(r#"DELETE FROM "Template" WHERE "Id" = ?"#)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerFnError::new(format!("Template {id} does not exist.")));
    }
    Ok(())
}

#[server]
pub async fn get_template(id: i64) -> Result<Option<TemplateForm>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let row = sqlx::query(
        r#"SELECT t."Id",
                  t."TemplateGroupId",
                  COALESCE(tg."Name", '') AS "GroupName",
                  COALESCE(t."Name", '') AS "Name"
           FROM "Template" t
           JOIN "TemplateGroup" tg ON tg."Id" = t."TemplateGroupId"
           WHERE t."Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(row.map(|row| TemplateForm {
        id: row.try_get("Id").unwrap_or_default(),
        group_id: row.try_get("TemplateGroupId").unwrap_or_default(),
        group_name: row.try_get("GroupName").unwrap_or_default(),
        name: row.try_get("Name").unwrap_or_default(),
    }))
}

#[server]
pub async fn get_template_items(template_id: i64) -> Result<Vec<TemplateItemForm>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    get_template_items_with_pool(&state.pool, template_id).await
}

#[cfg(feature = "ssr")]
pub async fn get_template_items_with_pool(
    pool: &DbPool,
    template_id: i64,
) -> Result<Vec<TemplateItemForm>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT ti."BlockId",
                  ti."StartTime",
                  COALESCE(b."Name", '') AS "BlockName",
                  b."Minutes"
           FROM "TemplateItem" ti
           JOIN "Block" b ON b."Id" = ti."BlockId"
           WHERE ti."TemplateId" = ?
           ORDER BY ti."StartTime", ti."Id""#,
    )
    .bind(template_id)
    .fetch_all(pool)
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let start =
            parse_time_to_minutes(&row.try_get::<String, _>("StartTime").unwrap_or_default())
                .unwrap_or_default();
        let duration = row.try_get::<i32, _>("Minutes").unwrap_or_default();
        let end = start + duration;
        if start < end || end == 24 * 60 {
            items.push(TemplateItemForm {
                block_id: row.try_get("BlockId").unwrap_or_default(),
                block_name: row.try_get("BlockName").unwrap_or_default(),
                start_minutes: start,
                duration_minutes: duration,
            });
        }
    }
    Ok(items)
}

#[server]
pub async fn list_template_block_groups() -> Result<Vec<TemplateBlockGroupOption>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "BlockGroup"
           ORDER BY "Name", "Id""#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| TemplateBlockGroupOption {
            id: row.try_get("Id").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn list_template_blocks_for_group(
    group_id: i64,
) -> Result<Vec<TemplateBlockOption>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id", "BlockGroupId", COALESCE("Name", '') AS "Name", "Minutes"
           FROM "Block"
           WHERE "BlockGroupId" = ?
           ORDER BY "Name", "Id""#,
    )
    .bind(group_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| TemplateBlockOption {
            id: row.try_get("Id").unwrap_or_default(),
            group_id: row.try_get("BlockGroupId").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
            minutes: row.try_get("Minutes").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn save_template_with_items(form: TemplateWithItemsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    save_template_with_pool(&state.pool, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_template_with_pool(
    pool: &DbPool,
    form: TemplateWithItemsForm,
) -> Result<(), ServerFnError> {
    validate_template_exists(pool, form.id).await?;
    validate_template_group_exists(pool, form.group_id).await?;
    let name = validate_template_name(pool, form.group_id, Some(form.id), &form.name).await?;
    validate_template_items(pool, &form.items).await?;

    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"UPDATE "Template"
           SET "Name" = ?, "DateUpdated" = CURRENT_TIMESTAMP
           WHERE "Id" = ?"#,
    )
    .bind(&name)
    .bind(form.id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(r#"DELETE FROM "TemplateItem" WHERE "TemplateId" = ?"#)
        .bind(form.id)
        .execute(&mut *tx)
        .await?;

    for item in &form.items {
        sqlx::query(
            r#"INSERT INTO "TemplateItem" ("TemplateId", "BlockId", "StartTime")
               VALUES (?, ?, ?)"#,
        )
        .bind(form.id)
        .bind(item.block_id)
        .bind(format_minutes(item.start_minutes))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_template_group_name(pool: &DbPool, name: &str) -> Result<String, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Template group name is required"));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new(
            "Template group name must be 50 characters or fewer",
        ));
    }
    let duplicate: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TemplateGroup" WHERE "Name" = ?"#)
            .bind(name)
            .fetch_one(pool)
            .await?;
    if duplicate > 0 {
        return Err(ServerFnError::new("Template group name must be unique"));
    }
    Ok(name.to_string())
}

#[cfg(feature = "ssr")]
async fn validate_template_group_exists(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TemplateGroup" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Template group does not exist."));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_template_exists(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Template" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("[TemplateId] does not exist."));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_template_name(
    pool: &DbPool,
    group_id: i64,
    template_id: Option<i64>,
    name: &str,
) -> Result<String, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Template name is required"));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new(format!(
            "Template name \"{name}\" is invalid"
        )));
    }
    let duplicate: i64 = match template_id {
        Some(id) => {
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Template"
                   WHERE "TemplateGroupId" = ? AND "Name" = ? AND "Id" != ?"#,
            )
            .bind(group_id)
            .bind(name)
            .bind(id)
            .fetch_one(pool)
            .await?
        }
        None => {
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Template"
                   WHERE "TemplateGroupId" = ? AND "Name" = ?"#,
            )
            .bind(group_id)
            .bind(name)
            .fetch_one(pool)
            .await?
        }
    };
    if duplicate > 0 {
        return Err(ServerFnError::new(format!(
            "A template named \"{name}\" already exists in that template group"
        )));
    }
    Ok(name.to_string())
}

#[cfg(feature = "ssr")]
async fn validate_template_items(
    pool: &DbPool,
    items: &[TemplateItemForm],
) -> Result<(), ServerFnError> {
    use std::collections::HashMap;

    if items.is_empty() {
        return Ok(());
    }

    let mut block_minutes = HashMap::new();
    for block_id in items.iter().map(|item| item.block_id) {
        if block_minutes.contains_key(&block_id) {
            continue;
        }
        let minutes: Option<i32> =
            sqlx::query_scalar(r#"SELECT "Minutes" FROM "Block" WHERE "Id" = ?"#)
                .bind(block_id)
                .fetch_optional(pool)
                .await?;
        let Some(minutes) = minutes else {
            return Err(ServerFnError::new("[BlockId] does not exist."));
        };
        block_minutes.insert(block_id, minutes);
    }

    let mut ranges = Vec::with_capacity(items.len());
    for item in items {
        let duration = block_minutes
            .get(&item.block_id)
            .copied()
            .unwrap_or(item.duration_minutes)
            .max(0);
        let start = item.start_minutes.clamp(0, 24 * 60);
        let end = start + duration;
        ranges.push((item.block_id, start, end));
    }

    for (index, (_, start, end)) in ranges.iter().enumerate() {
        if *end > 24 * 60 {
            return Err(ServerFnError::new("Template items cannot cross midnight."));
        }
        for (_, other_start, other_end) in ranges.iter().skip(index + 1) {
            if *start < *other_end && *other_start < *end {
                return Err(ServerFnError::new(format!(
                    "Block from {} to {} intersects block from {} to {}",
                    format_minutes(*start),
                    format_minutes(*end),
                    format_minutes(*other_start),
                    format_minutes(*other_end)
                )));
            }
        }
    }

    Ok(())
}

#[must_use]
pub fn format_minutes(minutes: i32) -> String {
    let clamped = minutes.clamp(0, 24 * 60);
    let hours = clamped / 60;
    let minutes = clamped % 60;
    format!("{hours:02}:{minutes:02}:00")
}

#[must_use]
pub fn format_short_time(minutes: i32) -> String {
    let clamped = minutes.clamp(0, 24 * 60);
    let hours = clamped / 60;
    let minutes = clamped % 60;
    format!("{hours:02}:{minutes:02}")
}

#[must_use]
pub fn parse_time_to_minutes(value: &str) -> Option<i32> {
    let mut parts = value.trim().split(':');
    let hours = parts.next()?.parse::<i32>().ok()?;
    let minutes = parts.next()?.parse::<i32>().ok()?;
    if !(0..=24).contains(&hours) || !(0..=59).contains(&minutes) {
        return None;
    }
    let total = hours * 60 + minutes;
    (total <= 24 * 60).then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_values_round_trip_for_template_items() {
        assert_eq!(parse_time_to_minutes("08:35:00"), Some(515));
        assert_eq!(parse_time_to_minutes("24:00"), Some(1440));
        assert_eq!(format_minutes(515), "08:35:00");
        assert_eq!(format_short_time(515), "08:35");
    }

    #[test]
    fn invalid_time_values_are_rejected() {
        assert_eq!(parse_time_to_minutes("24:05"), None);
        assert_eq!(parse_time_to_minutes("bad"), None);
    }
}
