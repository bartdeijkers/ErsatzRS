//! Rebuild derived state from an accepted SQL read before indexed session startup.
use std::{path::Path, time::Instant};

use chrono::{DateTime, Utc};
use ersatzrs_core::programme::ProgrammeClock;

use super::{
    contract::{MAX_INDEX_BYTES, MAX_TOTAL_BYTES},
    files, planner, PublicationError, INDEX_FILENAME,
};
use crate::next_engine::protocol::Playout;

// Ordinary publication retains two generations. Startup may also have a
// stranded candidate; refuse further growth rather than deleting rollback data
// before accepted SQL is visible. Retry reuses any already-correct chunks.
const MAX_RECOVERY_BYTES: u64 = (3 * MAX_TOTAL_BYTES + 4 * MAX_INDEX_BYTES) as u64;

pub(in crate::next_engine::publication) fn recover(
    folder: &Path,
    playout: &Playout,
    clock: &ProgrammeClock,
    now: DateTime<Utc>,
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
    committed: impl FnOnce(),
) -> Result<(), PublicationError> {
    super::super::validate(playout)?;
    super::check(is_current, deadline)?;
    files::prepare_directory(folder)?;
    let lease = files::exclusive_readers(folder)?;
    let inventory = files::recovery_inventory(folder, is_current, deadline)?;
    peak_bytes(inventory.bytes, 0)?;
    // Never parse a corrupt/candidate old index to select the startup schedule.
    // The coordinator's committed read and held source locks are authoritative.
    let plan = planner::plan(playout, None, clock, now, is_current, deadline)?
        .ok_or(PublicationError::InvalidIndex)?;
    let mut chunks = files::CreatedChunks::default();
    let mut footprint = inventory.bytes;
    for (reference, bytes) in plan.chunks {
        super::check(is_current, deadline)?;
        let old_size = match files::inspect_recovery_chunk(folder, &reference, &bytes)? {
            files::RecoveryChunk::Current => continue,
            files::RecoveryChunk::Missing => None,
            files::RecoveryChunk::Replace(size) => Some(size),
        };
        let peak = peak_bytes(footprint, bytes.len())?;
        files::repair_chunk(
            &folder.join(&reference.file),
            &bytes,
            old_size.is_none(),
            &mut chunks,
            is_current,
            deadline,
        )?;
        footprint = peak - old_size.unwrap_or(0);
    }
    peak_bytes(footprint, 0)?;
    let mut index = super::super::prepare_with(
        folder,
        |file| {
            serde_json::to_writer(file, &plan.index).map_err(|_| PublicationError::Serialization)
        },
        is_current,
        deadline,
    )?;
    index.destination = folder.join(INDEX_FILENAME);
    index.publish(is_current)?;
    // Cleanup may fail or be cancelled after publication. Preserve referenced
    // chunks and leave startup unready until a committed-read retry finishes.
    chunks.retain();
    committed();
    files::finish_recovery(inventory.entries, &plan.index, &lease, is_current, deadline)?;
    super::check(is_current, deadline)
}

fn peak_bytes(footprint: u64, additional: usize) -> Result<u64, PublicationError> {
    let peak = footprint
        .checked_add(additional as u64)
        .ok_or(PublicationError::Limit)?;
    if peak
        .checked_add(MAX_INDEX_BYTES as u64)
        .is_none_or(|size| size > MAX_RECOVERY_BYTES)
    {
        return Err(PublicationError::Limit);
    }
    Ok(peak)
}

#[cfg(test)]
#[path = "indexed_recovery_tests.rs"]
mod tests;
