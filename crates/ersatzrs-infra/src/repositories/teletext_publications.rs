//! Immutable pending requests; this adapter never activates teletext output.

use crate::xmltv::{
    capture_guide_snapshot, GuideExpansionLimits, GuideReadBudget, GuideReadLimits,
    GuideSnapshotDigestLimits,
};
use async_trait::async_trait;
use ersatzrs_core::programme::ProgrammeClock;
use ersatzrs_core::repositories::{
    RepositoryError, Result, TeletextGuideSourceCheckLimits, TeletextGuideSourceFence,
    TeletextProfileMutation, TeletextPublicationMutation as M, TeletextPublicationRejection as R,
    TeletextPublicationRepository, TeletextPublicationRequestSave,
};
use ersatzrs_domain::{EntityId, TeletextPublicationRequest, MAX_TELETEXT_PAGE_RECORDS};
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use sqlx::{Row as _, Transaction};
use std::{fmt, num::NonZeroU8, time::Instant};

use super::{
    teletext_designs::{head_in_transaction, revision_in_transaction},
    teletext_profiles::channel_snapshot_in_transaction,
    SqlxResultExt as _,
};
use crate::db::{Db, DbPool, DbRow};

/// Transactional pending request history and exact channel current-pointer CAS.
/// Stored intent carries no checked-page or active-publication authority.
#[derive(Clone)]
pub struct SqliteTeletextPublicationRepository(pub DbPool);

#[path = "teletext_effective_publications.rs"]
pub(super) mod effective;

#[path = "teletext_effective_scope.rs"]
pub(super) mod effective_scope;

#[async_trait]
impl TeletextPublicationRepository for SqliteTeletextPublicationRepository {
    async fn request_with_source_held(
        &self,
        save: &TeletextPublicationRequestSave,
        fence: &TeletextGuideSourceFence,
        check: &ersatzrs_core::repositories::TeletextGuideGenerationCheck,
        max_document_bytes: usize,
    ) -> Result<M<TeletextPublicationRequest>> {
        self.request_checked(
            save,
            max_document_bytes,
            Some(SourceCheck {
                fence,
                limits: check.limits,
                deadline: check.deadline,
                hold: Some(check.hold.clone()),
            }),
        )
        .await
    }

    async fn request_effective_with_source_held(
        &self,
        save: &ersatzrs_core::repositories::TeletextEffectivePublicationRequestSave,
        fence: &TeletextGuideSourceFence,
        check: &ersatzrs_core::repositories::TeletextGuideGenerationCheck,
        max_document_bytes: usize,
    ) -> Result<M<TeletextPublicationRequest>> {
        effective::request(
            &self.0,
            save,
            max_document_bytes,
            check.deadline,
            Some(SourceCheck {
                fence,
                limits: check.limits,
                deadline: check.deadline,
                hold: Some(check.hold.clone()),
            }),
        )
        .await
    }

    async fn request_effective(
        &self,
        save: &ersatzrs_core::repositories::TeletextEffectivePublicationRequestSave,
        max_document_bytes: usize,
        deadline: Instant,
    ) -> Result<M<TeletextPublicationRequest>> {
        effective::request(&self.0, save, max_document_bytes, deadline, None).await
    }

    async fn request_effective_with_source(
        &self,
        save: &ersatzrs_core::repositories::TeletextEffectivePublicationRequestSave,
        fence: &TeletextGuideSourceFence,
        limits: TeletextGuideSourceCheckLimits,
        max_document_bytes: usize,
        deadline: Instant,
    ) -> Result<M<TeletextPublicationRequest>> {
        effective::request(
            &self.0,
            save,
            max_document_bytes,
            deadline,
            Some(SourceCheck {
                fence,
                limits,
                deadline,
                hold: None,
            }),
        )
        .await
    }

    async fn current_effective(
        &self,
        channel_id: EntityId,
    ) -> Result<Option<TeletextPublicationRequest>> {
        if channel_id <= 0 {
            return Err(invalid_identity());
        }
        // Read-only transaction: role-specific pending metadata grants no output.
        let mut tx = self.0.begin().await.repository()?;
        effective::current_in_transaction(&mut tx, channel_id).await
    }

    async fn request(
        &self,
        save: &TeletextPublicationRequestSave,
        max_document_bytes: usize,
    ) -> Result<M<TeletextPublicationRequest>> {
        self.request_checked(save, max_document_bytes, None).await
    }

    async fn request_with_source(
        &self,
        save: &TeletextPublicationRequestSave,
        fence: &TeletextGuideSourceFence,
        limits: TeletextGuideSourceCheckLimits,
        max_document_bytes: usize,
        deadline: Instant,
    ) -> Result<M<TeletextPublicationRequest>> {
        self.request_checked(
            save,
            max_document_bytes,
            Some(SourceCheck {
                fence,
                limits,
                deadline,
                hold: None,
            }),
        )
        .await
    }

    async fn current(&self, channel_id: EntityId) -> Result<Option<TeletextPublicationRequest>> {
        if channel_id <= 0 {
            return Err(invalid_identity());
        }
        // Read-only transaction: pending pointer projection never promotes output.
        let mut tx = self.0.begin().await.repository()?;
        current_in_transaction(&mut tx, channel_id).await
    }
}

struct SourceCheck<'a> {
    fence: &'a TeletextGuideSourceFence,
    limits: TeletextGuideSourceCheckLimits,
    deadline: Instant,
    hold: Option<ersatzrs_core::locking::TeletextWorkHold>,
}

impl SqliteTeletextPublicationRepository {
    async fn request_checked(
        &self,
        save: &TeletextPublicationRequestSave,
        max_document_bytes: usize,
        check: Option<SourceCheck<'_>>,
    ) -> Result<M<TeletextPublicationRequest>> {
        if !valid_save(save) {
            return Ok(M::Rejected(R::InvalidValues));
        }
        let deadline = check.as_ref().map(|check| check.deadline);
        let admission = self.admit_request(save, max_document_bytes, check);
        let admitted = if let Some(deadline) = deadline {
            match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), admission).await
            {
                Ok(result) => result?,
                Err(_) => return Ok(M::Rejected(R::GuideTimedOut)),
            }
        } else {
            admission.await?
        };
        match admitted {
            Ok(tx) => insert_request(tx, save).await,
            Err(reason) => Ok(M::Rejected(reason)),
        }
    }

    async fn admit_request(
        &self,
        save: &TeletextPublicationRequestSave,
        max_document_bytes: usize,
        check: Option<SourceCheck<'_>>,
    ) -> Result<std::result::Result<Transaction<'static, Db>, R>> {
        let deadline = check.as_ref().map(|check| check.deadline);
        if let Some(check) = &check {
            if Instant::now() >= check.deadline {
                return Ok(Err(R::GuideTimedOut));
            }
        }
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
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
            revision_in_transaction(&mut tx, save.design_id, save.revision, max_document_bytes)
                .await?
        else {
            return Ok(Err(R::MissingRevision));
        };
        if revision.document_version != ersatzrs_domain::TELETEXT_DESIGN_DOCUMENT_VERSION {
            // Opaque storage also retains effective originals. Only the separate
            // scoped role may qualify those; they cannot become base requests.
            return Ok(Err(R::InvalidValues));
        }
        if revision.digest != save.digest {
            return Ok(Err(R::DigestConflict));
        }
        match channel_snapshot_in_transaction(&mut tx, save.selection.channel_id).await? {
            TeletextProfileMutation::Rejected(reason) => {
                return Ok(Err(R::SelectionUnavailable(reason)));
            }
            TeletextProfileMutation::Saved(current) if current != save.selection => {
                return Ok(Err(R::SelectionChanged));
            }
            TeletextProfileMutation::Saved(_) => {}
        }
        let current = current_in_transaction(&mut tx, save.selection.channel_id).await?;
        if current.as_ref().map(|request| request.id) != save.expected_request_id {
            return Ok(Err(R::RequestConflict));
        }
        let Ok(stored) = stored_fence(&revision.document) else {
            return Ok(Err(R::GuideUnavailable));
        };
        match (stored, check) {
            (Some(_), None) => return Ok(Err(R::GuideFenceRequired)),
            (None, Some(_)) => return Ok(Err(R::GuideFenceMismatch)),
            (Some(stored), Some(check)) => {
                if stored != *check.fence {
                    return Ok(Err(R::GuideFenceMismatch));
                }
                if let Err(reason) = check_source(&mut tx, check).await {
                    return Ok(Err(reason));
                }
            }
            (None, None) => {}
        }
        drop(revision);
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Ok(Err(R::GuideTimedOut));
        }
        // The timeout covers only admission. Once this transaction is returned,
        // INSERT/commit retain the established database completion contract.
        Ok(Ok(tx))
    }
}

async fn insert_request(
    mut tx: Transaction<'_, Db>,
    save: &TeletextPublicationRequestSave,
) -> Result<M<TeletextPublicationRequest>> {
    let result = sqlx::query(
        r#"INSERT INTO "TeletextPublicationRequest"
            ("ChannelId", "DesignId", "Revision", "Digest") VALUES (?, ?, ?, ?)"#,
    )
    .bind(save.selection.channel_id)
    .bind(save.design_id)
    .bind(save.revision)
    .bind(&save.digest)
    .execute(&mut *tx)
    .await
    .repository()?;
    let id = result.last_insert_rowid();
    sqlx::query(
            r#"INSERT INTO "TeletextPublicationRequestCurrent" ("ChannelId", "RequestId")
            VALUES (?, ?) ON CONFLICT("ChannelId") DO UPDATE SET "RequestId" = excluded."RequestId""#,
        )
        .bind(save.selection.channel_id)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
    tx.commit().await.repository()?;
    Ok(M::Requested(TeletextPublicationRequest {
        id,
        channel_id: save.selection.channel_id,
        design_id: save.design_id,
        revision: save.revision,
        digest: save.digest.clone(),
    }))
}

async fn check_source(
    tx: &mut Transaction<'_, Db>,
    check: SourceCheck<'_>,
) -> std::result::Result<(), R> {
    check_source_inner(tx, check, false).await.map(|_| ())
}

/// Reuse the same transaction-owned source capture/hash, independently deriving
/// original-capture current-role validity from its bounded native occurrences.
pub(super) async fn check_current_source(
    tx: &mut Transaction<'_, Db>,
    fence: &TeletextGuideSourceFence,
    check: &ersatzrs_core::repositories::TeletextGuideGenerationCheck,
) -> std::result::Result<
    ersatzrs_core::repositories::TeletextGuideGenerationQualification,
    ersatzrs_core::repositories::TeletextGenerationRejection,
> {
    use ersatzrs_core::repositories::TeletextGenerationRejection as G;
    let checked = check_source_inner(
        tx,
        SourceCheck {
            fence,
            limits: check.limits,
            deadline: check.deadline,
            hold: Some(check.hold.clone()),
        },
        true,
    )
    .await
    .map_err(G::Publication)?;
    let qualification = checked
        .qualification
        .ok_or(G::GuideFreshnessRequired)?
        .map_err(|_| G::GuideFreshnessRequired)?;
    qualification
        .validate_current(chrono::Utc::now())
        .map_err(|_| G::GuideFreshnessRequired)?;
    Ok(qualification)
}

struct SourceProof {
    digest: [u8; 32],
    qualification: Option<
        std::result::Result<
            ersatzrs_core::repositories::TeletextGuideGenerationQualification,
            ersatzrs_core::repositories::TeletextGuideQualificationError,
        >,
    >,
}

async fn check_source_inner(
    tx: &mut Transaction<'_, Db>,
    check: SourceCheck<'_>,
    current_role: bool,
) -> std::result::Result<SourceProof, R> {
    if Instant::now() >= check.deadline {
        return Err(R::GuideTimedOut);
    }
    check.fence.validate().map_err(|_| R::GuideFenceMismatch)?;
    let clock = ProgrammeClock::system().map_err(|_| R::GuideUnavailable)?;
    if clock.iana_name() != Some(check.fence.timezone.as_str()) {
        return Err(R::GuideChanged);
    }
    let days = NonZeroU8::new(3).ok_or(R::GuideUnavailable)?;
    let bounds = clock
        .calendar_day_bounds(check.fence.captured_at, days)
        .map_err(|_| R::GuideUnavailable)?;
    if bounds != (check.fence.start, check.fence.finish) {
        return Err(R::GuideChanged);
    }
    let mut budget = GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: check.limits.max_records,
            max_source_bytes: check.limits.max_source_bytes,
        },
        GuideExpansionLimits {
            max_nodes: check.limits.max_model_nodes,
            max_bytes: check.limits.max_model_bytes,
        },
    );
    let snapshot = tokio::time::timeout(
        check.deadline.saturating_duration_since(Instant::now()),
        capture_guide_snapshot(
            tx,
            &check.fence.channel_ids,
            check.fence.start,
            check.fence.finish,
            &mut budget,
        ),
    )
    .await
    .map_err(|_| R::GuideTimedOut)?
    .map_err(|_| R::GuideUnavailable)?;
    if Instant::now() >= check.deadline {
        return Err(R::GuideTimedOut);
    }
    let limits = GuideSnapshotDigestLimits {
        max_fields: check.limits.max_digest_fields,
        max_bytes: check.limits.max_digest_bytes,
    };
    let deadline = check.deadline;
    let digest_version = check.fence.context_digest_version;
    let current_source = current_role.then(|| check.fence.clone());
    let hold = check.hold.clone();
    // Move the exact transaction-owned capture into a joined bounded CPU worker.
    // Caller cancellation does not turn a partial hash into successful admission.
    let mut task = PendingHash(tokio::task::spawn_blocking(move || {
        let _hold = hold;
        if Instant::now() >= deadline {
            return Err(R::GuideTimedOut);
        }
        let digest = snapshot
            .context_digest_for_version(digest_version, limits)
            .map_err(|_| R::GuideUnavailable)?;
        if Instant::now() >= deadline {
            return Err(R::GuideTimedOut);
        }
        let qualification = current_source.map(|source| {
            source.qualify_current(
                snapshot
                    .channels()
                    .iter()
                    .flat_map(crate::xmltv::CapturedGuideChannel::occurrences),
                chrono::Utc::now(),
            )
        });
        if Instant::now() >= deadline {
            return Err(R::GuideTimedOut);
        }
        Ok(SourceProof {
            digest,
            qualification,
        })
    }));
    let actual = (&mut task.0).await.map_err(|_| R::GuideUnavailable)??;
    if Instant::now() >= check.deadline {
        return Err(R::GuideTimedOut);
    }
    if actual.digest != check.fence.context_digest {
        return Err(R::GuideChanged);
    }
    Ok(actual)
}

struct PendingHash(tokio::task::JoinHandle<std::result::Result<SourceProof, R>>);
impl Drop for PendingHash {
    fn drop(&mut self) {
        // Deadline/caller cancellation aborts queued CPU work. Already running
        // blocking work is bounded, checks the same deadline and has no DB handle.
        self.0.abort();
    }
}

struct FenceRoot;
impl<'de> DeserializeSeed<'de> for FenceRoot {
    type Value = Option<TeletextGuideSourceFence>;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for FenceRoot {
    type Value = Option<TeletextGuideSourceFence>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a saved original document object")
    }
    fn visit_map<A: MapAccess<'de>>(
        self,
        mut map: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        let mut fence = None;
        while let Some(key) = map.next_key::<String>()? {
            if key == "guide_source" {
                if fence.is_some() {
                    return Err(serde::de::Error::custom("duplicate guide-source receipt"));
                }
                fence = Some(map.next_value::<TeletextGuideSourceFence>()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(fence)
    }
}

pub(super) fn stored_fence(
    bytes: &[u8],
) -> std::result::Result<Option<TeletextGuideSourceFence>, ()> {
    // Old repository-only opaque fixtures remain valid static documents. JSON
    // starters must form a complete root map; malformed JSON is never absence.
    let first = bytes
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .copied();
    if !first.is_some_and(|byte| {
        matches!(
            byte,
            b'{' | b'[' | b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n'
        )
    }) {
        return Ok(None);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let fence = FenceRoot.deserialize(&mut deserializer).map_err(|_| ())?;
    deserializer.end().map_err(|_| ())?;
    Ok(fence)
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn valid_save(save: &TeletextPublicationRequestSave) -> bool {
    save.design_id > 0
        && save.revision > 0
        && save.expected_design_head > 0
        && save.expected_request_id.is_none_or(|id| id > 0)
        && save.selection.channel_id > 0
        && !save.selection.bindings.is_empty()
        && save.selection.bindings.len() <= MAX_TELETEXT_PAGE_RECORDS
        && valid_digest(&save.digest)
}

pub(super) async fn current_in_transaction(
    tx: &mut Transaction<'_, Db>,
    channel_id: EntityId,
) -> Result<Option<TeletextPublicationRequest>> {
    // A dangling or malformed pointer is an error, not a missing request.
    // Numeric CASE guards also exclude oversized corrupt TEXT/BLOB metadata.
    let row = sqlx::query(
        r#"SELECT
        CASE WHEN TYPEOF(p."RequestId") = 'integer' THEN p."RequestId" ELSE NULL END AS "Pointer",
        CASE WHEN TYPEOF(r."Id") = 'integer' THEN r."Id" ELSE NULL END AS "Id",
        CASE WHEN TYPEOF(r."ChannelId") = 'integer' THEN r."ChannelId" ELSE NULL END AS "ChannelId",
        CASE WHEN TYPEOF(r."DesignId") = 'integer' THEN r."DesignId" ELSE NULL END AS "DesignId",
        CASE WHEN TYPEOF(r."Revision") = 'integer' THEN r."Revision" ELSE NULL END AS "Revision",
        CASE WHEN TYPEOF(r."Digest") = 'text' AND LENGTH(CAST(r."Digest" AS BLOB)) = 64
            THEN r."Digest" ELSE NULL END AS "Digest"
        FROM "TeletextPublicationRequestCurrent" p LEFT JOIN "TeletextPublicationRequest" r
            ON r."Id" = p."RequestId" AND r."ChannelId" = p."ChannelId"
        WHERE p."ChannelId" = ?"#,
    )
    .bind(channel_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    row.as_ref()
        .map(|row| decode_request(row, channel_id))
        .transpose()
}

fn decode_request(row: &DbRow, channel_id: EntityId) -> Result<TeletextPublicationRequest> {
    let number = |column: &str| -> Result<i64> {
        row.try_get::<Option<i64>, _>(column)
            .repository()?
            .filter(|value| *value > 0)
            .ok_or_else(corrupt)
    };
    let id = number("Id")?;
    let owner = number("ChannelId")?;
    let digest: Option<String> = row.try_get("Digest").repository()?;
    let digest = digest
        .filter(|digest| valid_digest(digest))
        .ok_or_else(corrupt)?;
    if number("Pointer")? != id || owner != channel_id {
        return Err(corrupt());
    }
    Ok(TeletextPublicationRequest {
        id,
        channel_id: owner,
        design_id: number("DesignId")?,
        revision: number("Revision")?,
        digest,
    })
}

fn invalid_identity() -> RepositoryError {
    RepositoryError::Validation {
        field: "channel identity",
        message: "invalid teletext request identity".to_owned(),
    }
}

fn corrupt() -> RepositoryError {
    RepositoryError::Decode {
        kind: "teletext publication request",
        value: "invalid stored pending request".to_owned(),
    }
}

#[cfg(test)]
#[path = "teletext_publications_tests.rs"]
pub(super) mod tests;
