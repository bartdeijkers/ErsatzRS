//! Transaction-owned, accepted-only native guide capture.
//! All source occurrences are grouped before applying the half-open window.
//! Budgets model admitted inputs and additional allocations, not total RSS.

use crate::{
    db::{Db, DbRow},
    xmltv_read::{GuideExpansionUsage, GuideReadBudget, GuideReadUsage, Read},
};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::{
    programme::{project_occurrences, ProgrammeOccurrence},
    xmltv::XmltvBlockBehavior,
};
use ersatzrs_domain::{ChannelPlayoutSource, PlayoutScheduleKind};
use sqlx::Row;

#[path = "xmltv_snapshot/allocation.rs"]
mod allocation;
#[path = "xmltv_snapshot/digest.rs"]
mod digest;
pub use digest::{
    GuideSnapshotDigestError, GuideSnapshotDigestLimits, GUIDE_SNAPSHOT_DIGEST_VERSION,
};
#[path = "xmltv_snapshot/sources.rs"]
mod sources;

#[cfg(test)]
#[path = "xmltv_snapshot/tests.rs"]
mod tests;

/// Source identity captured preparatorily; never publication/freshness authority.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GuideSnapshotSource {
    Sql {
        playout_id: i64,
        schedule_kind: PlayoutScheduleKind,
        raw_rows: usize,
    },
    Accepted {
        playout_id: i64,
        revision: i64,
        raw_programs: usize,
        /// Exact admitted accepted document bytes, including unprojected data.
        document_digest: [u8; 32],
    },
}

impl std::fmt::Debug for GuideSnapshotSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql {
                playout_id,
                schedule_kind,
                raw_rows,
            } => formatter
                .debug_struct("Sql")
                .field("playout_id", playout_id)
                .field("schedule_kind", schedule_kind)
                .field("raw_rows", raw_rows)
                .finish(),
            Self::Accepted {
                playout_id,
                revision,
                raw_programs,
                ..
            } => formatter
                .debug_struct("Accepted")
                .field("playout_id", playout_id)
                .field("revision", revision)
                .field("raw_programs", raw_programs)
                .finish_non_exhaustive(),
        }
    }
}

/// Exact stored path associations, without live-file or freshness authority.
#[derive(PartialEq, Eq)]
pub struct GuideSnapshotPathAssociation {
    playout_id: i64,
    configured_schedule_file: Option<String>,
    accepted_schedule_file: Option<String>,
}
impl GuideSnapshotPathAssociation {
    #[must_use]
    pub const fn playout_id(&self) -> i64 {
        self.playout_id
    }
    #[must_use]
    pub fn configured_schedule_file(&self) -> Option<&str> {
        self.configured_schedule_file.as_deref()
    }
    #[must_use]
    pub fn accepted_schedule_file(&self) -> Option<&str> {
        self.accepted_schedule_file.as_deref()
    }
}
impl std::fmt::Debug for GuideSnapshotPathAssociation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuideSnapshotPathAssociation")
            .field("playout_id", &self.playout_id)
            .field("configured", &self.configured_schedule_file.is_some())
            .field("accepted", &self.accepted_schedule_file.is_some())
            .finish_non_exhaustive()
    }
}

/// One exact selected channel; names and programme fields are not Debug.
pub struct CapturedGuideChannel {
    id: i64,
    number: String,
    name: String,
    playout_source: ChannelPlayoutSource,
    source_channel_id: i64,
    playout_offset: Duration,
    occurrences: Vec<ProgrammeOccurrence>,
    sources: Vec<GuideSnapshotSource>,
    path_associations: Vec<GuideSnapshotPathAssociation>,
}
impl CapturedGuideChannel {
    fn same_context(&self, other: &Self) -> bool {
        self.id == other.id
            && self.number == other.number
            && self.name == other.name
            && self.playout_source == other.playout_source
            && self.source_channel_id == other.source_channel_id
            && self.playout_offset == other.playout_offset
            && self.sources == other.sources
            && self.path_associations == other.path_associations
            && self.occurrences == other.occurrences
    }
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub fn number(&self) -> &str {
        &self.number
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn playout_source(&self) -> ChannelPlayoutSource {
        self.playout_source
    }
    #[must_use]
    pub const fn source_channel_id(&self) -> i64 {
        self.source_channel_id
    }
    #[must_use]
    pub const fn playout_offset(&self) -> Duration {
        self.playout_offset
    }
    #[must_use]
    pub fn occurrences(&self) -> &[ProgrammeOccurrence] {
        &self.occurrences
    }
    #[must_use]
    pub fn sources(&self) -> &[GuideSnapshotSource] {
        &self.sources
    }
    #[must_use]
    pub fn path_associations(&self) -> &[GuideSnapshotPathAssociation] {
        &self.path_associations
    }
}

/// Owned originals and projected occurrences from one transaction.
pub struct GuideSnapshot {
    channels: Vec<CapturedGuideChannel>,
    page_language: Option<String>,
    raw_block_behavior: Option<String>,
    block_behavior: XmltvBlockBehavior,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    source_usage: GuideReadUsage,
    expansion_usage: GuideExpansionUsage,
}

/// First semantic difference between captured contexts; never source freshness.
/// Channel order is significant. An absent expected channel reports its original
/// ID; an additional channel reports the additional ID. No content is disclosed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideSnapshotComparison {
    Unchanged,
    GlobalChanged,
    ChannelChanged(i64),
}

impl GuideSnapshot {
    /// Fingerprint exactly the fields compared by `compare_context`.
    ///
    /// The versioned streaming encoding preserves optional values, ordered
    /// collections and full metadata. Admission usage is excluded. This is
    /// preparatory data, not raw-source freshness or publication authority.
    ///
    /// # Errors
    /// Rejects checked arithmetic or trusted field/encoded-byte limits before
    /// hashing the next field. No partial digest is returned.
    pub fn context_digest(
        &self,
        limits: GuideSnapshotDigestLimits,
    ) -> Result<[u8; 32], GuideSnapshotDigestError> {
        digest::context_digest(self, limits)
    }

    /// Fingerprint with an explicitly selected historical framing version.
    /// Version 1 retains its exact semantic framing; version 2 additionally
    /// includes the raw accepted-document fingerprints. Neither is authority.
    /// # Errors
    /// Rejects unsupported versions, checked arithmetic or trusted work limits.
    pub fn context_digest_for_version(
        &self,
        version: u16,
        limits: GuideSnapshotDigestLimits,
    ) -> Result<[u8; 32], GuideSnapshotDigestError> {
        digest::context_digest_for_version(self, version, limits)
    }

    /// Compare the frozen window, raw/effective configuration and ordered channel
    /// context, including exact source receipts and complete occurrences.
    /// Admission usage is excluded; underlying uncaptured sources are not fenced.
    #[must_use]
    pub fn compare_context(&self, other: &Self) -> GuideSnapshotComparison {
        if self.start != other.start
            || self.finish != other.finish
            || self.page_language != other.page_language
            || self.raw_block_behavior != other.raw_block_behavior
            || self.block_behavior != other.block_behavior
        {
            return GuideSnapshotComparison::GlobalChanged;
        }
        for (index, channel) in self.channels.iter().enumerate() {
            if !other
                .channels
                .get(index)
                .is_some_and(|other| channel.same_context(other))
            {
                return GuideSnapshotComparison::ChannelChanged(channel.id);
            }
        }
        other
            .channels
            .get(self.channels.len())
            .map_or(GuideSnapshotComparison::Unchanged, |channel| {
                GuideSnapshotComparison::ChannelChanged(channel.id)
            })
    }
    #[must_use]
    pub fn channels(&self) -> &[CapturedGuideChannel] {
        &self.channels
    }
    #[must_use]
    pub fn channel(&self, id: i64) -> Option<&CapturedGuideChannel> {
        self.channels.iter().find(|c| c.id == id)
    }
    #[must_use]
    pub fn page_language(&self) -> Option<&str> {
        self.page_language.as_deref()
    }
    #[must_use]
    pub fn raw_block_behavior(&self) -> Option<&str> {
        self.raw_block_behavior.as_deref()
    }
    #[must_use]
    pub const fn block_behavior(&self) -> XmltvBlockBehavior {
        self.block_behavior
    }
    #[must_use]
    pub const fn start(&self) -> DateTime<Utc> {
        self.start
    }
    #[must_use]
    pub const fn finish(&self) -> DateTime<Utc> {
        self.finish
    }
    #[must_use]
    pub const fn source_usage(&self) -> GuideReadUsage {
        self.source_usage
    }
    #[must_use]
    pub const fn expansion_usage(&self) -> GuideExpansionUsage {
        self.expansion_usage
    }
}
impl std::fmt::Debug for GuideSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuideSnapshot")
            .field("channels", &self.channels.len())
            .field("source_usage", &self.source_usage)
            .field("expansion_usage", &self.expansion_usage)
            .finish_non_exhaustive()
    }
}

/// Static/numeric diagnostics never expose names, paths or stored document text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("guide snapshot capture failed: {reason} (channel {channel_id:?}, playout {playout_id:?})")]
pub struct GuideSnapshotError {
    pub channel_id: Option<i64>,
    pub playout_id: Option<i64>,
    pub reason: GuideSnapshotFailure,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GuideSnapshotFailure {
    #[error("expansion policy is required")]
    MissingExpansion,
    #[error("window is empty or reversed")]
    InvalidWindow,
    #[error("channel is missing")]
    MissingChannel,
    #[error("channel selection is duplicated")]
    DuplicateChannel,
    #[error("accepted external timeline is missing")]
    MissingAccepted,
    #[error("database access failed")]
    Database,
    #[error("source or expansion admission failed")]
    Admission,
    #[error("stored values are invalid")]
    InvalidValue,
    #[error("accepted external timeline is invalid: {0}")]
    Accepted(crate::external_json::snapshot::AcceptedSnapshotError),
}

fn failure(
    channel_id: Option<i64>,
    playout_id: Option<i64>,
    reason: GuideSnapshotFailure,
) -> GuideSnapshotError {
    GuideSnapshotError {
        channel_id,
        playout_id,
        reason,
    }
}
fn admitted<T>(
    result: Result<T, sqlx::Error>,
    channel: Option<i64>,
    playout: Option<i64>,
) -> Result<T, GuideSnapshotError> {
    result.map_err(|error| {
        failure(
            channel,
            playout,
            match error {
                sqlx::Error::Decode(_) => GuideSnapshotFailure::Admission,
                sqlx::Error::ColumnDecode { .. } | sqlx::Error::ColumnNotFound(_) => {
                    GuideSnapshotFailure::InvalidValue
                }
                _ => GuideSnapshotFailure::Database,
            },
        )
    })
}
async fn rows(
    transaction: &mut sqlx::Transaction<'_, Db>,
    budget: &mut GuideReadBudget,
    sql: &str,
    ids: &[i64],
    columns: &[&str],
    channel: Option<i64>,
    playout: Option<i64>,
) -> Result<Vec<DbRow>, GuideSnapshotError> {
    admitted(
        Read::snapshot(transaction, budget)
            .fetch_rows(sql, ids, columns)
            .await,
        channel,
        playout,
    )
}

/// Capture selected channels without changing legacy XMLTV/file readers.
/// Hidden channels remain inspectable. Every external source must already have
/// an accepted revision; mutable-file capture/recheck is a separate open policy.
/// The caller owns the encompassing transaction, deadline and channel selection.
///
/// # Errors
/// Requires expansion admission and rejects missing/corrupt accepted sources,
/// unknown/duplicate channels, invalid windows/values and aggregate budgets.
pub async fn capture_guide_snapshot(
    transaction: &mut sqlx::Transaction<'_, Db>,
    channel_ids: &[i64],
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    budget: &mut GuideReadBudget,
) -> Result<GuideSnapshot, GuideSnapshotError> {
    budget
        .require_expansion()
        .map_err(|_| failure(None, None, GuideSnapshotFailure::MissingExpansion))?;
    if finish <= start {
        return Err(failure(None, None, GuideSnapshotFailure::InvalidWindow));
    }
    admitted(
        Read::snapshot(transaction, budget).preflight_identifiers(channel_ids.len()),
        None,
        None,
    )?;
    for (index, id) in channel_ids.iter().enumerate() {
        if channel_ids[..index].contains(id) {
            return Err(failure(
                Some(*id),
                None,
                GuideSnapshotFailure::DuplicateChannel,
            ));
        }
    }
    let (page_language, raw_block_behavior, block_behavior) =
        configuration(transaction, budget).await?;
    admitted(
        allocation::slots::<CapturedGuideChannel>(budget, channel_ids.len()),
        None,
        None,
    )?;
    let mut channels = Vec::with_capacity(channel_ids.len());
    for &id in channel_ids {
        channels
            .push(capture_channel(transaction, budget, id, start, finish, block_behavior).await?);
    }
    Ok(GuideSnapshot {
        channels,
        page_language,
        raw_block_behavior,
        block_behavior,
        start,
        finish,
        source_usage: budget.usage(),
        expansion_usage: budget
            .expansion_usage()
            .ok_or_else(|| failure(None, None, GuideSnapshotFailure::MissingExpansion))?,
    })
}

async fn configuration(
    transaction: &mut sqlx::Transaction<'_, Db>,
    budget: &mut GuideReadBudget,
) -> Result<(Option<String>, Option<String>, XmltvBlockBehavior), GuideSnapshotError> {
    let values=rows(transaction,budget,r#"SELECT "Key","Value" FROM "ConfigElement" WHERE "Key" IN ('pages.language','xmltv.block_behavior') ORDER BY "Key""#,
        &[],&["Key","Value"],None,None).await?;
    let mut language = None;
    let mut raw_block = None;
    for row in values {
        let key: String = admitted(row.try_get("Key"), None, None)?;
        if key == "pages.language" {
            language = admitted(row.try_get("Value"), None, None)?;
        } else {
            let value: String = admitted(row.try_get("Value"), None, None)?;
            raw_block = Some(value);
        }
    }
    if let Some(value) = &raw_block {
        admitted(budget.reserve_expansion(1, value.trim().len()), None, None)?;
    }
    let block = raw_block
        .as_deref()
        .and_then(XmltvBlockBehavior::parse)
        .unwrap_or_default();
    Ok((language, raw_block, block))
}

async fn capture_channel(
    transaction: &mut sqlx::Transaction<'_, Db>,
    budget: &mut GuideReadBudget,
    id: i64,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    behavior: XmltvBlockBehavior,
) -> Result<CapturedGuideChannel, GuideSnapshotError> {
    let selected=rows(transaction,budget,r#"SELECT "Id","Number","Name","PlayoutSource","MirrorSourceChannelId","PlayoutOffset" FROM "Channel" WHERE "Id"=?"#,
        &[id],&["Id","Number","Name","PlayoutSource","MirrorSourceChannelId","PlayoutOffset"],Some(id),None).await?;
    let row = selected
        .into_iter()
        .next()
        .ok_or_else(|| failure(Some(id), None, GuideSnapshotFailure::MissingChannel))?;
    // The retained source SELECT uses numeric CASE=1, so named TEXT must not
    // acquire a different mirror interpretation through generic enum decoding.
    let _: i32 = admitted(row.try_get("PlayoutSource"), Some(id), None)?;
    let source: ChannelPlayoutSource = crate::repositories::common::decode_enum_column(
        "guide playout source",
        &row,
        "PlayoutSource",
    )
    .map_err(|_| failure(Some(id), None, GuideSnapshotFailure::InvalidValue))?;
    let source_id = if source == ChannelPlayoutSource::Mirror {
        admitted(
            row.try_get::<Option<i64>, _>("MirrorSourceChannelId"),
            Some(id),
            None,
        )?
        .ok_or_else(|| failure(Some(id), None, GuideSnapshotFailure::InvalidValue))?
    } else {
        id
    };
    let offset = if source == ChannelPlayoutSource::Mirror {
        let value: Option<String> = admitted(row.try_get("PlayoutOffset"), Some(id), None)?;
        value
            .as_deref()
            .map(crate::repositories::common::try_decode_signed_duration_text)
            .transpose()
            .map_err(|_| failure(Some(id), None, GuideSnapshotFailure::InvalidValue))?
            .unwrap_or_default()
    } else {
        Duration::zero()
    };
    let mut channel = CapturedGuideChannel {
        id,
        number: admitted(row.try_get("Number"), Some(id), None)?,
        name: admitted(row.try_get("Name"), Some(id), None)?,
        playout_source: source,
        source_channel_id: source_id,
        playout_offset: offset,
        occurrences: Vec::new(),
        sources: Vec::new(),
        path_associations: Vec::new(),
    };
    let items = sources::load(transaction, budget, &mut channel, source_id).await?;
    admitted(allocation::core_projection(budget, &items), Some(id), None)?;
    let projected = project_occurrences(&items, behavior);
    admitted(
        allocation::slots::<ProgrammeOccurrence>(budget, projected.len()),
        Some(id),
        None,
    )?;
    channel.occurrences = projected
        .into_iter()
        .filter(|item| item.programme.start < finish && item.programme.stop > start)
        .collect();
    Ok(channel)
}
