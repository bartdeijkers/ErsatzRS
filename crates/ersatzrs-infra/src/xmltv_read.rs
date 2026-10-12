//! Shared legacy or transaction-borrowed source reads for XMLTV metadata.
//! These bounds measure source rows/bytes, not later decoded/cloned content.

use sqlx::Row as _;

use crate::db::{assert_sql_safe, Db, DbPool, DbRow};

/// Trusted aggregate source limits for transaction-owned guide preparation.
/// Input identifier counts also obey `max_records` before cache allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuideReadLimits {
    /// Aggregate selected rows and the ceiling for each input identifier list.
    pub max_records: usize,
    /// Sum of every selected value cast to BLOB, including corrupt scalar fields.
    pub max_source_bytes: usize,
}

/// Source costs admitted before retrieval through a shared read budget.
/// These are not retained-content, decoded collection or projection costs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GuideReadUsage {
    pub records: usize,
    pub source_bytes: usize,
}

/// One caller-owned source budget; rejection never yields a truncated result.
#[derive(Debug)]
pub struct GuideReadBudget {
    limits: GuideReadLimits,
    usage: GuideReadUsage,
}

impl GuideReadBudget {
    #[must_use]
    pub const fn new(limits: GuideReadLimits) -> Self {
        Self {
            limits,
            usage: GuideReadUsage {
                records: 0,
                source_bytes: 0,
            },
        }
    }

    #[must_use]
    pub const fn usage(&self) -> GuideReadUsage {
        self.usage
    }

    fn charge(&mut self, records: usize, source_bytes: usize) -> Result<(), sqlx::Error> {
        let next = GuideReadUsage {
            records: self
                .usage
                .records
                .checked_add(records)
                .ok_or_else(arithmetic)?,
            source_bytes: self
                .usage
                .source_bytes
                .checked_add(source_bytes)
                .ok_or_else(arithmetic)?,
        };
        check("records", next.records, self.limits.max_records)?;
        check(
            "source bytes",
            next.source_bytes,
            self.limits.max_source_bytes,
        )?;
        self.usage = next;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
enum GuideReadBudgetError {
    #[error("guide source {category} count {count} exceeds limit {maximum}")]
    Limit {
        category: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("guide source size arithmetic is invalid")]
    Arithmetic,
}

fn arithmetic() -> sqlx::Error {
    sqlx::Error::Decode(Box::new(GuideReadBudgetError::Arithmetic))
}

fn check(category: &'static str, count: usize, maximum: usize) -> Result<(), sqlx::Error> {
    if count > maximum {
        return Err(sqlx::Error::Decode(Box::new(GuideReadBudgetError::Limit {
            category,
            count,
            maximum,
        })));
    }
    Ok(())
}

pub(crate) enum Read<'a> {
    Legacy(&'a DbPool),
    Snapshot {
        connection: &'a mut sqlx::SqliteConnection,
        budget: &'a mut GuideReadBudget,
    },
}

impl<'a> Read<'a> {
    pub(crate) const fn legacy(pool: &'a DbPool) -> Self {
        Self::Legacy(pool)
    }

    pub(crate) fn snapshot(
        transaction: &'a mut sqlx::Transaction<'_, Db>,
        budget: &'a mut GuideReadBudget,
    ) -> Self {
        Self::Snapshot {
            connection: transaction.as_mut(),
            budget,
        }
    }

    pub(crate) fn preflight_identifiers(&self, count: usize) -> Result<(), sqlx::Error> {
        if let Self::Snapshot { budget, .. } = self {
            check("identifiers", count, budget.limits.max_records)?;
        }
        Ok(())
    }

    /// Only repository-owned SELECTs and explicitly selected payload aliases.
    /// Preflight and retrieval use identical SQL/ID bindings and one connection.
    pub(crate) async fn fetch_rows(
        &mut self,
        sql: &str,
        ids: &[i64],
        payload_columns: &[&str],
    ) -> Result<Vec<DbRow>, sqlx::Error> {
        let mut query = sqlx::query(assert_sql_safe(sql));
        for id in ids {
            query = query.bind(*id);
        }
        match self {
            Self::Legacy(pool) => query.fetch_all(*pool).await,
            Self::Snapshot { connection, budget } => {
                let (records, source_bytes) =
                    measure(connection, sql, ids, payload_columns).await?;
                budget.charge(records, source_bytes)?;
                query.fetch_all(&mut **connection).await
            }
        }
    }
}

async fn measure(
    connection: &mut sqlx::SqliteConnection,
    sql: &str,
    ids: &[i64],
    payload_columns: &[&str],
) -> Result<(usize, usize), sqlx::Error> {
    // Aliases are explicit trusted repository constants, never input SQL/fields.
    let bytes = if payload_columns.is_empty() {
        "0".to_owned()
    } else {
        payload_columns
            .iter()
            .map(|column| format!("COALESCE(length(CAST(\"{column}\" AS BLOB)),0)"))
            .collect::<Vec<_>>()
            .join("+")
    };
    let measured = format!(
        "SELECT COUNT(*) AS records, COALESCE(SUM({bytes}),0) AS source_bytes FROM ({sql}) AS source"
    );
    let mut query = sqlx::query(assert_sql_safe(measured));
    for id in ids {
        query = query.bind(*id);
    }
    let row = query.fetch_one(connection).await?;
    let records: i64 = row.try_get("records")?;
    let source_bytes: i64 = row.try_get("source_bytes")?;
    Ok((
        usize::try_from(records).map_err(|_| arithmetic())?,
        usize::try_from(source_bytes).map_err(|_| arithmetic())?,
    ))
}
