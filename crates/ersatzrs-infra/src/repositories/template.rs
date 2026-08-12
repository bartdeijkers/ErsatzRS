use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{
    RepositoryError, Result, TemplateAdminRow, TemplateBlockAdmin, TemplateBlockGroupAdmin,
    TemplateGroupAdminRow, TemplateItemAdmin, TemplateRepository,
};
use ersatzrs_domain::{EntityId, Template};
use sqlx::{Pool, Row};

use super::common::upsert_sql;
use super::common::{decode_string_column, load_ids, not_found};

#[derive(Clone)]
pub struct SqliteTemplateRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl TemplateRepository for SqliteTemplateRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Template> {
        let row = sqlx::query(r#"SELECT * FROM "Template" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("template", id, error))?;
        let mut template = map_template(&row)?;
        template.item_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "TemplateItem" WHERE "TemplateId" = ? ORDER BY "StartTime", "Id""#,
            id,
        )
        .await?;
        Ok(template)
    }

    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Template>> {
        let rows = sqlx::query(
            r#"SELECT * FROM "Template" WHERE "TemplateGroupId" = ? ORDER BY "Name", "Id""#,
        )
        .bind(group_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                let mut template = map_template(row)?;
                template.item_ids.clear();
                Ok(template)
            })
            .collect()
    }

    async fn upsert(&self, template: &Template) -> Result<()> {
        let mut tx = self.0.begin().await.repository()?;
        sqlx::query(upsert_sql!(
            r#"INSERT INTO "Template" (
                "Id", "TemplateGroupId", "Name", "DateUpdated"
             ) VALUES (?, ?, ?, ?)"#,
            &[&["Id"], &["TemplateGroupId", "Name"]],
        ))
        .bind(template.id)
        .bind(template.template_group_id)
        .bind(&template.name)
        .bind(template.updated_ticks.to_string())
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn list_groups_admin(&self) -> Result<Vec<TemplateGroupAdminRow>> {
        let rows = sqlx::query(
            r#"SELECT tg."Id", tg."Name", COUNT(t."Id") AS "TemplateCount"
               FROM "TemplateGroup" tg
               LEFT JOIN "Template" t ON t."TemplateGroupId" = tg."Id"
               GROUP BY tg."Id", tg."Name"
               ORDER BY tg."Name", tg."Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(TemplateGroupAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: decode_string_column(row, "Name")?,
                    template_count: row.try_get("TemplateCount").repository()?,
                })
            })
            .collect()
    }

    async fn list_admin(&self, search: &str) -> Result<Vec<TemplateAdminRow>> {
        let search = search.trim();
        let like = format!("%{search}%");
        let rows = sqlx::query(
            r#"SELECT t."Id", t."TemplateGroupId", tg."Name" AS "GroupName", t."Name"
               FROM "Template" t
               JOIN "TemplateGroup" tg ON tg."Id" = t."TemplateGroupId"
               WHERE (? = '' OR t."Name" LIKE ?)
               UNION ALL
               SELECT -tg."Id" AS "Id", tg."Id" AS "TemplateGroupId",
                      tg."Name" AS "GroupName", '(none)' AS "Name"
               FROM "TemplateGroup" tg
               WHERE ? = ''
                 AND NOT EXISTS (
                     SELECT 1 FROM "Template" t WHERE t."TemplateGroupId" = tg."Id"
                 )
               ORDER BY "GroupName", "Name""#,
        )
        .bind(search)
        .bind(like)
        .bind(search)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter().map(map_template_admin).collect()
    }

    async fn group_exists_admin(&self, id: EntityId) -> Result<bool> {
        exists_by_id(&self.0, "TemplateGroup", id).await
    }

    async fn template_exists_admin(&self, id: EntityId) -> Result<bool> {
        exists_by_id(&self.0, "Template", id).await
    }

    async fn group_name_exists_admin(&self, name: &str) -> Result<bool> {
        let count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TemplateGroup" WHERE "Name" = ?"#)
                .bind(name)
                .fetch_one(&self.0)
                .await
                .repository()?;
        Ok(count > 0)
    }

    async fn template_name_exists_admin(
        &self,
        group_id: EntityId,
        template_id: Option<EntityId>,
        name: &str,
    ) -> Result<bool> {
        let count: i64 = match template_id {
            Some(id) => sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Template"
                   WHERE "TemplateGroupId" = ? AND "Name" = ? AND "Id" <> ?"#,
            )
            .bind(group_id)
            .bind(name)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .repository()?,
            None => sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Template"
                   WHERE "TemplateGroupId" = ? AND "Name" = ?"#,
            )
            .bind(group_id)
            .bind(name)
            .fetch_one(&self.0)
            .await
            .repository()?,
        };
        Ok(count > 0)
    }

    async fn create_group_admin(&self, name: &str) -> Result<EntityId> {
        let result = sqlx::query(r#"INSERT INTO "TemplateGroup" ("Name") VALUES (?)"#)
            .bind(name)
            .execute(&self.0)
            .await
            .repository()?;
        Ok(crate::db::last_insert_id(result))
    }

    async fn create_admin(&self, group_id: EntityId, name: &str) -> Result<EntityId> {
        let result = sqlx::query(
            r#"INSERT INTO "Template" ("TemplateGroupId", "Name", "DateUpdated")
               VALUES (?, ?, CURRENT_TIMESTAMP)"#,
        )
        .bind(group_id)
        .bind(name)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(crate::db::last_insert_id(result))
    }

    async fn copy_admin(
        &self,
        template_id: EntityId,
        new_group_id: EntityId,
        name: &str,
    ) -> Result<EntityId> {
        let mut transaction = self.0.begin().await.repository()?;
        let result = sqlx::query(
            r#"INSERT INTO "Template" ("TemplateGroupId", "Name", "DateUpdated")
               SELECT ?, ?, CURRENT_TIMESTAMP
               FROM "Template" WHERE "Id" = ?"#,
        )
        .bind(new_group_id)
        .bind(name)
        .bind(template_id)
        .execute(&mut *transaction)
        .await
        .repository()?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "template",
                id: template_id,
            });
        }
        let new_id = crate::db::last_insert_id(result);
        sqlx::query(
            r#"INSERT INTO "TemplateItem" ("TemplateId", "BlockId", "StartTime")
               SELECT ?, "BlockId", "StartTime"
               FROM "TemplateItem" WHERE "TemplateId" = ?"#,
        )
        .bind(new_id)
        .bind(template_id)
        .execute(&mut *transaction)
        .await
        .repository()?;
        transaction.commit().await.repository()?;
        Ok(new_id)
    }

    async fn delete_group_admin(&self, id: EntityId) -> Result<()> {
        delete_by_id(&self.0, "TemplateGroup", "template group", id).await
    }

    async fn delete_admin(&self, id: EntityId) -> Result<()> {
        delete_by_id(&self.0, "Template", "template", id).await
    }

    async fn get_admin(&self, id: EntityId) -> Result<Option<TemplateAdminRow>> {
        sqlx::query(
            r#"SELECT t."Id", t."TemplateGroupId", tg."Name" AS "GroupName", t."Name"
               FROM "Template" t
               JOIN "TemplateGroup" tg ON tg."Id" = t."TemplateGroupId"
               WHERE t."Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .as_ref()
        .map(map_template_admin)
        .transpose()
    }

    async fn list_items_admin(&self, template_id: EntityId) -> Result<Vec<TemplateItemAdmin>> {
        let rows = sqlx::query(
            r#"SELECT ti."BlockId", ti."StartTime", b."Name" AS "BlockName", b."Minutes"
               FROM "TemplateItem" ti
               JOIN "Block" b ON b."Id" = ti."BlockId"
               WHERE ti."TemplateId" = ?
               ORDER BY ti."StartTime", ti."Id""#,
        )
        .bind(template_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                let start = parse_time_to_minutes(&decode_string_column(row, "StartTime")?)
                    .ok_or_else(|| RepositoryError::Decode {
                        kind: "template item start time",
                        value: decode_string_column(row, "StartTime")
                            .unwrap_or_else(|_| "<invalid>".to_owned()),
                    })?;
                Ok(TemplateItemAdmin {
                    block_id: row.try_get("BlockId").repository()?,
                    block_name: decode_string_column(row, "BlockName")?,
                    start_minutes: start,
                    duration_minutes: row.try_get("Minutes").repository()?,
                })
            })
            .collect()
    }

    async fn list_block_groups_admin(&self) -> Result<Vec<TemplateBlockGroupAdmin>> {
        let rows = sqlx::query(r#"SELECT "Id", "Name" FROM "BlockGroup" ORDER BY "Name", "Id""#)
            .fetch_all(&self.0)
            .await
            .repository()?;
        rows.iter()
            .map(|row| {
                Ok(TemplateBlockGroupAdmin {
                    id: row.try_get("Id").repository()?,
                    name: decode_string_column(row, "Name")?,
                })
            })
            .collect()
    }

    async fn list_blocks_admin(&self, group_id: EntityId) -> Result<Vec<TemplateBlockAdmin>> {
        let rows = sqlx::query(
            r#"SELECT "Id", "BlockGroupId", "Name", "Minutes"
               FROM "Block" WHERE "BlockGroupId" = ?
               ORDER BY "Name", "Id""#,
        )
        .bind(group_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(TemplateBlockAdmin {
                    id: row.try_get("Id").repository()?,
                    group_id: row.try_get("BlockGroupId").repository()?,
                    name: decode_string_column(row, "Name")?,
                    minutes: row.try_get("Minutes").repository()?,
                })
            })
            .collect()
    }

    async fn save_admin(
        &self,
        id: EntityId,
        name: &str,
        items: &[TemplateItemAdmin],
    ) -> Result<()> {
        let mut transaction = self.0.begin().await.repository()?;
        let result = sqlx::query(
            r#"UPDATE "Template" SET "Name" = ?, "DateUpdated" = CURRENT_TIMESTAMP
               WHERE "Id" = ?"#,
        )
        .bind(name)
        .bind(id)
        .execute(&mut *transaction)
        .await
        .repository()?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "template",
                id,
            });
        }
        sqlx::query(r#"DELETE FROM "TemplateItem" WHERE "TemplateId" = ?"#)
            .bind(id)
            .execute(&mut *transaction)
            .await
            .repository()?;
        for item in items {
            sqlx::query(
                r#"INSERT INTO "TemplateItem" ("TemplateId", "BlockId", "StartTime")
                   VALUES (?, ?, ?)"#,
            )
            .bind(id)
            .bind(item.block_id)
            .bind(format_minutes(item.start_minutes))
            .execute(&mut *transaction)
            .await
            .repository()?;
        }
        transaction.commit().await.repository()?;
        Ok(())
    }
}

fn map_template_admin(row: &DbRow) -> Result<TemplateAdminRow> {
    Ok(TemplateAdminRow {
        id: row.try_get("Id").repository()?,
        group_id: row.try_get("TemplateGroupId").repository()?,
        group_name: decode_string_column(row, "GroupName")?,
        name: decode_string_column(row, "Name")?,
    })
}

async fn exists_by_id(
    pool: &Pool<crate::db::Db>,
    table: &'static str,
    id: EntityId,
) -> Result<bool> {
    let query = match table {
        "TemplateGroup" => r#"SELECT COUNT(*) FROM "TemplateGroup" WHERE "Id" = ?"#,
        "Template" => r#"SELECT COUNT(*) FROM "Template" WHERE "Id" = ?"#,
        _ => unreachable!("validated template table"),
    };
    let count: i64 = sqlx::query_scalar(query)
        .bind(id)
        .fetch_one(pool)
        .await
        .repository()?;
    Ok(count > 0)
}

async fn delete_by_id(
    pool: &Pool<crate::db::Db>,
    table: &'static str,
    kind: &'static str,
    id: EntityId,
) -> Result<()> {
    let query = match table {
        "TemplateGroup" => r#"DELETE FROM "TemplateGroup" WHERE "Id" = ?"#,
        "Template" => r#"DELETE FROM "Template" WHERE "Id" = ?"#,
        _ => unreachable!("validated template table"),
    };
    let result = sqlx::query(query)
        .bind(id)
        .execute(pool)
        .await
        .repository()?;
    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound { kind, id });
    }
    Ok(())
}

fn parse_time_to_minutes(value: &str) -> Option<i32> {
    let mut parts = value.trim().split(':');
    let hours = parts.next()?.parse::<i32>().ok()?;
    let minutes = parts.next()?.parse::<i32>().ok()?;
    if !(0..=24).contains(&hours) || !(0..=59).contains(&minutes) {
        return None;
    }
    let total = hours * 60 + minutes;
    (total <= 24 * 60).then_some(total)
}

fn format_minutes(minutes: i32) -> String {
    let clamped = minutes.clamp(0, 24 * 60);
    format!("{:02}:{:02}:00", clamped / 60, clamped % 60)
}

fn map_template(row: &DbRow) -> Result<Template> {
    Ok(Template {
        id: row.try_get("Id").repository()?,
        template_group_id: row.try_get("TemplateGroupId").repository()?,
        name: row.try_get("Name").repository()?,
        updated_ticks: row
            .try_get::<String, _>("DateUpdated")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0),
        item_ids: Vec::new(),
    })
}
