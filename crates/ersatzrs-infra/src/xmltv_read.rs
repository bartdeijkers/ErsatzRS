//! Shared legacy or transaction-borrowed source reads for XMLTV metadata.
//! Source and optional modeled expansion costs remain separate from full guide projection.

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

/// Additional metadata model/allocation costs beyond admitted source rows.
/// These are modeled costs, not an exact allocator or `SQLite` RSS measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuideExpansionLimits {
    pub max_nodes: usize,
    pub max_bytes: usize,
}

/// Cumulative admitted decoded-list, cache-capacity and copy costs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GuideExpansionUsage {
    pub nodes: usize,
    pub bytes: usize,
}

#[derive(Debug)]
struct Expansion {
    limits: GuideExpansionLimits,
    usage: GuideExpansionUsage,
}

/// One caller-owned source budget; rejection never yields a truncated result.
#[derive(Debug)]
pub struct GuideReadBudget {
    limits: GuideReadLimits,
    usage: GuideReadUsage,
    expansion: Option<Expansion>,
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
            expansion: None,
        }
    }

    /// Add cumulative decoded/copy limits while retaining the source policy.
    #[must_use]
    pub const fn new_with_expansion(
        limits: GuideReadLimits,
        expansion: GuideExpansionLimits,
    ) -> Self {
        let mut budget = Self::new(limits);
        budget.expansion = Some(Expansion {
            limits: expansion,
            usage: GuideExpansionUsage { nodes: 0, bytes: 0 },
        });
        budget
    }

    #[must_use]
    pub const fn usage(&self) -> GuideReadUsage {
        self.usage
    }

    /// `None` denotes the original raw-source-only policy.
    #[must_use]
    pub fn expansion_usage(&self) -> Option<GuideExpansionUsage> {
        self.expansion.as_ref().map(|budget| budget.usage)
    }

    pub(crate) const fn remaining_source_limits(&self) -> GuideReadLimits {
        // Successful admission always keeps usage within both immutable limits.
        GuideReadLimits {
            max_records: self.limits.max_records - self.usage.records,
            max_source_bytes: self.limits.max_source_bytes - self.usage.source_bytes,
        }
    }

    pub(crate) fn require_expansion(&self) -> Result<(), sqlx::Error> {
        if self.expansion.is_none() {
            return Err(sqlx::Error::Decode(
                "guide snapshot requires an explicit metadata expansion policy".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn charge_source(
        &mut self,
        records: usize,
        source_bytes: usize,
    ) -> Result<(), sqlx::Error> {
        self.charge(records, source_bytes)
    }

    pub(crate) fn reserve_expansion(
        &mut self,
        nodes: usize,
        bytes: usize,
    ) -> Result<(), sqlx::Error> {
        let Some(expansion) = &mut self.expansion else {
            return Ok(());
        };
        let next = GuideExpansionUsage {
            nodes: expansion
                .usage
                .nodes
                .checked_add(nodes)
                .ok_or_else(arithmetic)?,
            bytes: expansion
                .usage
                .bytes
                .checked_add(bytes)
                .ok_or_else(arithmetic)?,
        };
        check("expansion nodes", next.nodes, expansion.limits.max_nodes)?;
        check("expansion bytes", next.bytes, expansion.limits.max_bytes)?;
        expansion.usage = next;
        Ok(())
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

    pub(crate) fn expansion_enabled(&self) -> bool {
        match self {
            Self::Legacy(_) => false,
            Self::Snapshot { budget, .. } => budget.expansion.is_some(),
        }
    }

    pub(crate) fn reserve_expansion(
        &mut self,
        nodes: usize,
        bytes: usize,
    ) -> Result<(), sqlx::Error> {
        match self {
            Self::Legacy(_) => Ok(()),
            Self::Snapshot { budget, .. } => budget.reserve_expansion(nodes, bytes),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> GuideReadLimits {
        GuideReadLimits {
            max_records: 8,
            max_source_bytes: 32,
        }
    }

    #[test]
    fn expansion_limit_rejection_is_atomic_and_separate_from_source_admission() {
        let mut budget = GuideReadBudget::new_with_expansion(
            source(),
            GuideExpansionLimits {
                max_nodes: 2,
                max_bytes: 5,
            },
        );
        budget.charge(1, 3).unwrap();
        budget.reserve_expansion(1, 4).unwrap();
        let admitted = budget.expansion_usage();
        assert!(budget.reserve_expansion(1, 2).is_err());
        assert_eq!(budget.expansion_usage(), admitted);
        assert!(budget.reserve_expansion(2, 0).is_err());
        assert_eq!(budget.expansion_usage(), admitted);
        assert_eq!(
            budget.usage(),
            GuideReadUsage {
                records: 1,
                source_bytes: 3
            }
        );
        budget.reserve_expansion(1, 1).unwrap();
        assert_eq!(
            budget.expansion_usage(),
            Some(GuideExpansionUsage { nodes: 2, bytes: 5 })
        );
    }

    #[test]
    fn expansion_checked_overflow_does_not_commit_the_other_dimension() {
        for (nodes, bytes, next_nodes, next_bytes) in [(usize::MAX, 0, 1, 1), (1, usize::MAX, 1, 1)]
        {
            let mut budget = GuideReadBudget::new_with_expansion(
                source(),
                GuideExpansionLimits {
                    max_nodes: usize::MAX,
                    max_bytes: usize::MAX,
                },
            );
            budget.reserve_expansion(nodes, bytes).unwrap();
            assert!(budget.reserve_expansion(next_nodes, next_bytes).is_err());
            assert_eq!(
                budget.expansion_usage(),
                Some(GuideExpansionUsage { nodes, bytes })
            );
            assert_eq!(budget.usage(), GuideReadUsage::default());
        }
    }
}
