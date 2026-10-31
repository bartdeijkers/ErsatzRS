//! Separate pending pointers for exact published Scripted scopes, not output.

use super::*;
use chrono::Utc;
use ersatzrs_core::repositories::{
    TeletextEffectivePublicationRequestSave, TeletextProfileRejection,
};
use ersatzrs_domain::{
    TeletextEffectiveBindingSnapshot as Binding,
    TeletextEffectiveChannelSelectionSnapshot as Selection, MAX_TELETEXT_OVERRIDE_COMMANDS,
    TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
};

pub(in crate::repositories) fn valid_scope(selection: &Selection) -> bool {
    selection.channel_id > 0
        && !selection.bindings.is_empty()
        && selection.bindings.len() <= 2 * MAX_TELETEXT_OVERRIDE_COMMANDS
        && selection.valid_until.is_some()
        && selection
            .bindings
            .iter()
            .any(|binding| matches!(binding, Binding::Scripted(_)))
        && selection.bindings.iter().all(|binding| {
            binding.channel_id() == selection.channel_id
                && !binding.addon_id().is_empty()
                && binding.addon_id().len() <= 256
        })
        && selection
            .bindings
            .windows(2)
            .all(|pair| pair[0].addon_id() < pair[1].addon_id())
        && selection
            .bindings
            .iter()
            .filter(|binding| binding.enabled())
            .count()
            <= MAX_TELETEXT_PAGE_RECORDS
}

pub(in crate::repositories) fn expired(selection: &Selection) -> bool {
    selection
        .valid_until
        .is_none_or(|finish| Utc::now() >= finish)
}

/// Bound admission by the original transition as well as the supplied operation.
pub(in crate::repositories) fn scope_deadline(
    selection: &Selection,
    deadline: Instant,
) -> Option<Instant> {
    let observed = Instant::now();
    let remaining = (selection.valid_until? - Utc::now()).to_std().ok()?;
    Some(deadline.min(observed.checked_add(remaining)?))
}

pub(in crate::repositories) async fn check_scope(
    tx: &mut Transaction<'_, Db>,
    selection: &Selection,
) -> Result<std::result::Result<(), R>> {
    if expired(selection) {
        return Ok(Err(R::SelectionChanged));
    }
    Ok(
        match super::super::teletext_profiles::recheck_effective_in_transaction(
            tx,
            selection,
            Utc::now(),
        )
        .await?
        {
            TeletextProfileMutation::Saved(_) => Ok(()),
            TeletextProfileMutation::Rejected(TeletextProfileRejection::Conflict) => {
                Err(R::SelectionChanged)
            }
            TeletextProfileMutation::Rejected(reason) => Err(R::SelectionUnavailable(reason)),
        },
    )
}

pub(super) async fn request(
    pool: &DbPool,
    save: &TeletextEffectivePublicationRequestSave,
    maximum: usize,
    deadline: Instant,
    source: Option<SourceCheck<'_>>,
) -> Result<M<TeletextPublicationRequest>> {
    if save.design_id <= 0
        || save.revision <= 0
        || save.expected_design_head <= 0
        || save.expected_request_id.is_some_and(|id| id <= 0)
        || !valid_digest(&save.digest)
        || !valid_scope(&save.selection)
        || maximum == 0
        || isize::try_from(maximum).is_err()
    {
        return Ok(M::Rejected(R::InvalidValues));
    }
    if Instant::now() >= deadline {
        return Ok(M::Rejected(R::GuideTimedOut));
    }
    if expired(&save.selection) {
        return Ok(M::Rejected(R::SelectionChanged));
    }
    let Some(deadline) = scope_deadline(&save.selection, deadline) else {
        return Ok(M::Rejected(R::SelectionChanged));
    };
    let source = source.map(|source| SourceCheck { deadline, ..source });
    let admission = admit(pool, save, maximum, deadline, source);
    let tx =
        match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), admission).await {
            Err(_) => return Ok(M::Rejected(R::GuideTimedOut)),
            Ok(Err(error)) => return Err(error),
            Ok(Ok(Err(reason))) => return Ok(M::Rejected(reason)),
            Ok(Ok(Ok(tx))) => tx,
        };
    // Admission is bounded. Once admitted, writes/commit settle truthfully.
    insert(tx, save, deadline).await
}

async fn admit(
    pool: &DbPool,
    save: &TeletextEffectivePublicationRequestSave,
    maximum: usize,
    deadline: Instant,
    source: Option<SourceCheck<'_>>,
) -> Result<std::result::Result<Transaction<'static, Db>, R>> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let Some(head) = head_in_transaction(&mut tx, save.design_id).await? else {
        return Ok(Err(R::MissingDesign));
    };
    if head.deleted {
        return Ok(Err(R::DeletedDesign));
    }
    if head.revision != save.expected_design_head {
        return Ok(Err(R::DesignConflict));
    }
    let Some(revision) =
        revision_in_transaction(&mut tx, save.design_id, save.revision, maximum).await?
    else {
        return Ok(Err(R::MissingRevision));
    };
    if revision.document_version != TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION {
        return Ok(Err(R::InvalidValues));
    }
    if revision.digest != save.digest {
        return Ok(Err(R::DigestConflict));
    }
    if !super::effective_scope::matches(&revision.document, &save.selection) {
        return Ok(Err(R::SelectionChanged));
    }
    if let Err(reason) = check_scope(&mut tx, &save.selection).await? {
        return Ok(Err(reason));
    }
    let current = current_in_transaction(&mut tx, save.selection.channel_id).await?;
    if current.as_ref().map(|request| request.id) != save.expected_request_id {
        return Ok(Err(R::RequestConflict));
    }
    let Ok(stored) = stored_fence(&revision.document) else {
        return Ok(Err(R::GuideUnavailable));
    };
    match (stored, source) {
        (Some(_), None) => return Ok(Err(R::GuideFenceRequired)),
        (None, Some(_)) => return Ok(Err(R::GuideFenceMismatch)),
        (Some(stored), Some(source)) => {
            if stored != *source.fence {
                return Ok(Err(R::GuideFenceMismatch));
            }
            if let Err(reason) = check_source(&mut tx, source).await {
                return Ok(Err(reason));
            }
        }
        (None, None) => {}
    }
    if Instant::now() >= deadline {
        return Ok(Err(R::GuideTimedOut));
    }
    if let Err(reason) = check_scope(&mut tx, &save.selection).await? {
        return Ok(Err(reason));
    }
    Ok(Ok(tx))
}

async fn insert(
    mut tx: Transaction<'_, Db>,
    save: &TeletextEffectivePublicationRequestSave,
    deadline: Instant,
) -> Result<M<TeletextPublicationRequest>> {
    if Instant::now() >= deadline {
        return Ok(M::Rejected(R::GuideTimedOut));
    }
    if expired(&save.selection) {
        return Ok(M::Rejected(R::SelectionChanged));
    }
    let result = sqlx::query(r#"INSERT INTO "TeletextPublicationRequest" ("ChannelId","DesignId","Revision","Digest") VALUES (?,?,?,?)"#)
        .bind(save.selection.channel_id).bind(save.design_id).bind(save.revision).bind(&save.digest)
        .execute(&mut *tx).await.repository()?;
    let id = result.last_insert_rowid();
    sqlx::query(r#"INSERT INTO "TeletextEffectivePublicationRequestCurrent" ("ChannelId","RequestId") VALUES (?,?) ON CONFLICT("ChannelId") DO UPDATE SET "RequestId"=excluded."RequestId""#)
        .bind(save.selection.channel_id).bind(id).execute(&mut *tx).await.repository()?;
    tx.commit().await.repository()?;
    Ok(M::Requested(TeletextPublicationRequest {
        id,
        channel_id: save.selection.channel_id,
        design_id: save.design_id,
        revision: save.revision,
        digest: save.digest.clone(),
    }))
}

pub(in crate::repositories) async fn current_in_transaction(
    tx: &mut Transaction<'_, Db>,
    channel: EntityId,
) -> Result<Option<TeletextPublicationRequest>> {
    let row = sqlx::query(r#"SELECT
        CASE WHEN TYPEOF(p."RequestId")='integer' THEN p."RequestId" END AS "Pointer",
        CASE WHEN TYPEOF(r."Id")='integer' THEN r."Id" END AS "Id",
        CASE WHEN TYPEOF(r."ChannelId")='integer' THEN r."ChannelId" END AS "ChannelId",
        CASE WHEN TYPEOF(r."DesignId")='integer' THEN r."DesignId" END AS "DesignId",
        CASE WHEN TYPEOF(r."Revision")='integer' THEN r."Revision" END AS "Revision",
        CASE WHEN TYPEOF(r."Digest")='text' AND LENGTH(CAST(r."Digest" AS BLOB))=64 THEN r."Digest" END AS "Digest",
        CASE WHEN TYPEOF(d."DocumentVersion")='integer' THEN d."DocumentVersion" END AS "DocumentVersion",
        CASE WHEN TYPEOF(d."Digest")='text' AND LENGTH(CAST(d."Digest" AS BLOB))=64 THEN d."Digest" END AS "DesignDigest"
        FROM "TeletextEffectivePublicationRequestCurrent" p LEFT JOIN "TeletextPublicationRequest" r
            ON r."Id"=p."RequestId" AND r."ChannelId"=p."ChannelId"
        LEFT JOIN "TeletextDesignRevision" d ON d."DesignId"=r."DesignId" AND d."Revision"=r."Revision"
        WHERE p."ChannelId"=?"#).bind(channel).fetch_optional(&mut **tx).await.repository()?;
    row.as_ref()
        .map(|row| {
            let request = decode_request(row, channel)?;
            if row
                .try_get::<Option<i64>, _>("DocumentVersion")
                .repository()?
                != Some(i64::from(TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION))
                || row
                    .try_get::<Option<String>, _>("DesignDigest")
                    .repository()?
                    .as_ref()
                    != Some(&request.digest)
            {
                return Err(corrupt());
            }
            Ok(request)
        })
        .transpose()
}

#[cfg(test)]
#[path = "teletext_effective_publication_tests.rs"]
pub(in crate::repositories) mod tests;
