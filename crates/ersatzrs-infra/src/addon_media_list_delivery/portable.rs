//! ADR 0093: retain delivery cohorts without copying installation-local IDs.
//!
//! These transaction helpers are not a folder importer. The saved-content
//! coordinator owns validation, publication fencing, file authority and commit.

use std::collections::BTreeSet;

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{AddonMediaListDeliveryMode, EntityId};
use serde::{Deserialize, Serialize};
use sqlx::Transaction;

use crate::db::Db;
use crate::repositories::common::{decode_datetime, encode_datetime};
use crate::repositories::SqlxResultExt as _;

/// Saved delivery policy and provider identities on either side of its cutoff.
///
/// Includes identities whose item rows have been removed. For modes without a
/// cutoff, all known identities are in `before_cutoff` and `after_cutoff` is empty.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortableDeliveryPolicy {
    pub mode: AddonMediaListDeliveryMode,
    pub cutoff_at: Option<String>,
    pub before_cutoff: Vec<String>,
    pub after_cutoff: Vec<String>,
}

impl PortableDeliveryPolicy {
    pub(crate) fn validated(&self) -> Result<Self> {
        let mut normalized = self.clone();
        if self.mode == AddonMediaListDeliveryMode::DownloadAheadNewVideos {
            let at = self.cutoff_at.as_deref().ok_or_else(invalid)?;
            // Do not echo copied file content through a datetime decode error.
            let decoded = decode_datetime("delivery cutoff", at).map_err(|_| invalid())?;
            normalized.cutoff_at = Some(encode_datetime(decoded));
        } else if self.cutoff_at.is_some() || !self.after_cutoff.is_empty() {
            return Err(invalid());
        }
        let mut seen = BTreeSet::new();
        for identity in self.before_cutoff.iter().chain(&self.after_cutoff) {
            if identity.is_empty() || !seen.insert(identity) {
                return Err(invalid());
            }
        }
        normalized.before_cutoff.sort();
        normalized.after_cutoff.sort();
        Ok(normalized)
    }
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("saved delivery cohorts are invalid".to_owned())
}

/// Capture the full identity ledger using provider identities, never numeric IDs.
///
/// # Errors
/// Returns missing-list, malformed-policy or database errors.
pub async fn capture_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
) -> Result<PortableDeliveryPolicy> {
    let policy = super::load_in_tx(tx, list_id).await?;
    let known = sqlx::query_as::<_, (i64, String)>(
        r#"SELECT "Id", "ProviderId" FROM "AddonMediaListKnownIdentity"
           WHERE "AddonMediaListId" = ? ORDER BY "ProviderId""#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let mut result = PortableDeliveryPolicy {
        mode: policy.mode,
        cutoff_at: policy.cutoff_at,
        before_cutoff: Vec::new(),
        after_cutoff: Vec::new(),
    };
    for (id, provider_id) in known {
        if policy.cutoff_identity_id.is_some_and(|cutoff| id > cutoff) {
            result.after_cutoff.push(provider_id);
        } else {
            result.before_cutoff.push(provider_id);
        }
    }
    result.validated()
}

/// Restore a validated saved cohort with fresh local IDs in the caller's transaction.
///
/// The caller must hold its saved-content/publication fence and reconcile item
/// membership first. Every remaining item must occur in the incoming ledger.
/// Identical restoration is a no-op, including preservation of local identity IDs.
/// This changes only saved policy: immutable published selections stay unchanged.
/// The caller must roll back the transaction on any error.
///
/// # Errors
/// Rejects invalid cohorts, missing lists and omitted current identities; returns
/// database errors without committing partial work.
pub async fn restore_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    incoming: &PortableDeliveryPolicy,
) -> Result<()> {
    let incoming = incoming.validated()?;
    let current = capture_in_tx(tx, list_id).await?;
    let identities: BTreeSet<_> = incoming
        .before_cutoff
        .iter()
        .chain(&incoming.after_cutoff)
        .collect();
    let items = sqlx::query_scalar::<_, String>(
        r#"SELECT "ProviderId" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ? AND "InDraft" = 1"#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    if items.iter().any(|id| !identities.contains(id)) {
        return Err(invalid());
    }
    if current == incoming {
        return Ok(());
    }
    sqlx::query(r#"DELETE FROM "AddonMediaListKnownIdentity" WHERE "AddonMediaListId" = ?"#)
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    insert_identities(tx, list_id, &incoming.before_cutoff).await?;
    let cutoff: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX("Id"), 0) FROM "AddonMediaListKnownIdentity"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    insert_identities(tx, list_id, &incoming.after_cutoff).await?;
    let (mode, cutoff_id) = match incoming.mode {
        AddonMediaListDeliveryMode::Stream => (0, None),
        AddonMediaListDeliveryMode::DownloadAhead => (1, None),
        AddonMediaListDeliveryMode::DownloadAheadNewVideos => (2, Some(cutoff)),
    };
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "DeliveryMode" = ?, "DeliveryCutoffAt" = ?,
           "DeliveryCutoffIdentityId" = ? WHERE "Id" = ?"#,
    )
    .bind(mode)
    .bind(incoming.cutoff_at)
    .bind(cutoff_id)
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

async fn insert_identities(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    identities: &[String],
) -> Result<()> {
    for identity in identities {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListKnownIdentity" ("AddonMediaListId", "ProviderId")
               VALUES (?, ?)"#,
        )
        .bind(list_id)
        .bind(identity)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}
