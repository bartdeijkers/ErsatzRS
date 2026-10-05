//! Preserve explicit overrides when replacing existing schedule graphics joins.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_core::repositories::{validate_graphics_attachment_variables, Result};
use sqlx::{Pool, Row, Transaction};

use crate::{db::Db, repositories::SqlxResultExt as _};

#[derive(Clone, Copy)]
pub(super) enum Owner {
    Schedule,
    Block,
}

impl Owner {
    fn queries(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Schedule => (
                r#"SELECT "GraphicsElementId", "Variables" FROM "ProgramScheduleItemGraphicsElement" WHERE "ProgramScheduleItemId" = ? AND "Variables" IS NOT NULL ORDER BY "GraphicsElementId""#,
                r#"DELETE FROM "ProgramScheduleItemGraphicsElement" WHERE "ProgramScheduleItemId" = ?"#,
                r#"INSERT INTO "ProgramScheduleItemGraphicsElement" ("ProgramScheduleItemId", "GraphicsElementId", "Variables") VALUES (?, ?, ?)"#,
            ),
            Self::Block => (
                r#"SELECT "GraphicsElementId", "Variables" FROM "BlockItemGraphicsElement" WHERE "BlockItemId" = ? AND "Variables" IS NOT NULL ORDER BY "GraphicsElementId""#,
                r#"DELETE FROM "BlockItemGraphicsElement" WHERE "BlockItemId" = ?"#,
                r#"INSERT INTO "BlockItemGraphicsElement" ("BlockItemId", "GraphicsElementId", "Variables") VALUES (?, ?, ?)"#,
            ),
        }
    }
}

pub(super) async fn load(pool: &Pool<Db>, owner: Owner, id: i64) -> Result<BTreeMap<i64, String>> {
    let mut connection = pool.acquire().await.repository()?;
    load_on_connection(&mut connection, owner, id).await
}

pub(super) async fn load_on_connection(
    connection: &mut sqlx::SqliteConnection,
    owner: Owner,
    id: i64,
) -> Result<BTreeMap<i64, String>> {
    let (query, _, _) = owner.queries();
    let rows = sqlx::query(query)
        .bind(id)
        .fetch_all(&mut *connection)
        .await
        .repository()?;
    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("GraphicsElementId").repository()?,
                row.try_get("Variables").repository()?,
            ))
        })
        .collect()
}

pub(super) async fn replace(
    tx: &mut Transaction<'_, Db>,
    owner: Owner,
    id: i64,
    element_ids: &[i64],
    variables: &BTreeMap<i64, String>,
) -> Result<()> {
    let ids: BTreeSet<_> = element_ids.iter().copied().collect();
    validate_graphics_attachment_variables(element_ids, variables)?;
    let (_, delete, insert) = owner.queries();
    sqlx::query(delete)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for element in ids {
        sqlx::query(insert)
            .bind(id)
            .bind(element)
            .bind(variables.get(&element))
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(())
}
