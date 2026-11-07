//! ADR 0131: immutable daily chunks behind a single atomic visibility boundary.
use std::{path::Path, time::Instant};

use chrono::{DateTime, Utc};
use ersatzrs_core::programme::ProgrammeClock;

use super::{check_deadline, prepare_with, PreparedPlayout, PublicationError};
use crate::next_engine::protocol::Playout;

#[path = "indexed_contract.rs"]
mod contract;
#[path = "indexed_files.rs"]
mod files;
#[path = "indexed_planner.rs"]
mod planner;
#[path = "indexed_recovery.rs"]
mod recovery;
pub use contract::{
    DayReference, PlayoutIndex, INDEX_FILENAME, INDEX_VERSION, READER_LOCK_FILENAME,
};
pub(super) use recovery::recover;

pub(in crate::next_engine) struct PreparedIndex {
    index: Option<PreparedPlayout>,
    chunks: files::CreatedChunks,
    deadline: Option<Instant>,
}

/// Stage only changed day documents, then the complete index. The caller owns
/// channel/source admission through publication and any SQL compensation.
pub(super) fn prepare(
    folder: &Path,
    playout: &Playout,
    clock: &ProgrammeClock,
    now: DateTime<Utc>,
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
) -> Result<PreparedIndex, PublicationError> {
    super::validate(playout)?;
    check(is_current, deadline)?;
    files::prepare_directory(folder)?;
    let previous = files::load_previous(folder, now, is_current, deadline)?
        .ok_or(PublicationError::RecoveryRequired)?;
    let plan = planner::plan(playout, Some(&previous), clock, now, is_current, deadline)?;
    // Reclamation is serialized with readers only during deletion. No reader
    // lease is held across staged SQL or filesystem publication.
    files::reclaim(folder, Some(&previous), is_current, deadline)?;
    let mut chunks = files::CreatedChunks::default();
    let index = if let Some(plan) = plan {
        for (reference, bytes) in plan.chunks {
            check(is_current, deadline)?;
            files::stage_chunk(folder, &reference, &bytes, &mut chunks)?;
        }
        let mut index = prepare_with(
            folder,
            |file| {
                serde_json::to_writer(file, &plan.index)
                    .map_err(|_| PublicationError::Serialization)
            },
            is_current,
            deadline,
        )?;
        index.destination = folder.join(INDEX_FILENAME);
        Some(index)
    } else {
        None
    };
    Ok(PreparedIndex {
        index,
        chunks,
        deadline,
    })
}

pub(super) fn check(
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
) -> Result<(), PublicationError> {
    check_deadline(deadline)?;
    if !is_current() {
        return Err(PublicationError::Cancelled);
    }
    Ok(())
}

impl PreparedIndex {
    pub(super) fn restrict_deadline(&mut self, deadline: Instant) {
        self.deadline = Some(self.deadline.map_or(deadline, |old| old.min(deadline)));
        if let Some(index) = &mut self.index {
            index.restrict_deadline(deadline);
        }
    }

    pub(super) fn publish(mut self, is_current: impl Fn() -> bool) -> Result<(), PublicationError> {
        check(&is_current, self.deadline)?;
        if let Some(index) = self.index.take() {
            index.publish(is_current)?;
        }
        self.chunks.retain();
        Ok(())
    }

    pub(super) fn publish_reversible(
        mut self,
        is_current: impl Fn() -> bool,
    ) -> Result<Option<super::AppliedPlayout>, PublicationError> {
        check(&is_current, self.deadline)?;
        let applied = self
            .index
            .take()
            .map(|index| index.publish_reversible(is_current))
            .transpose()?;
        // An indexed reader may already have selected this generation. Keep its
        // chunks even after SQL rollback; the next admitted reclamation takes
        // the exclusive reader lease before deleting unreachable chunks.
        self.chunks.retain();
        Ok(applied)
    }
}

#[cfg(test)]
#[path = "indexed_publication_tests.rs"]
mod tests;
