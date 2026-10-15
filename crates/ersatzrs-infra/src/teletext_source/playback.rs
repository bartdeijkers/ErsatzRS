//! Read-only caption choices from exact stored local source observations.
//!
//! An actually observed header and a wire subtitle descriptor identify a
//! candidate, not a ready page. Separate descriptor discovery can request a
//! later current window without asserting that its page was observed. The
//! caller must verify current source bytes, actual bitmap decoding and
//! playback-worker ownership separately. Descriptor
//! tag 0x46 retains its provenance but never classifies a codec by itself; the
//! existing exact `dvb_teletext` stream/report association is required first.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    MediaSourceKind, SourceTeletextBinding, SourceTeletextObservation, SourceTeletextObservedPages,
    SourceTeletextPid, TeletextLanguage, TeletextPageNumber,
};
use ersatzrs_ffmpeg::teletext::source_descriptors::{
    parse_source_teletext_descriptors, SourceTeletextWireDescriptor,
};
use sqlx::Transaction;

use super::{column, load};
use crate::{
    db::{Db, DbPool},
    ffprobe::{
        teletext_decode::SourceTeletextPageDecodeRequest,
        teletext_probe::MAX_TELETEXT_PROBE_STREAMS,
    },
};

const MAX_CAPTION_CANDIDATES: usize = 1_024;

/// One explicit page/language/role on one bound stream and actual program.
/// Repeated equivalent descriptors remain visible as ordered provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTeletextCaptionCandidate {
    pub request: SourceTeletextPageDecodeRequest,
    pub language: TeletextLanguage,
    /// Wire role only; neither subtitle role establishes Default or Forced.
    pub descriptor_type: u8,
    pub psi_fingerprint: [u8; 32],
    pub descriptor_occurrences: Vec<SourceTeletextWireDescriptor>,
}

/// Descriptor-only request for held current-window preparation, never a choice.
///
/// The nested fields retain the exact stored binding, actual program and wire
/// provenance, but the requested page need not occur in the stored header
/// prefix. Before ranking, callers must independently require its current
/// same-byte page header, bitmap and actual playback-interval qualification.
/// This wrapper grants no stored readiness or `HeadersObserved` authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTeletextCaptionDescriptorRequest {
    pub candidate: SourceTeletextCaptionCandidate,
}

#[derive(Clone, Copy)]
enum DiscoveryMode {
    ObservedHeader,
    DescriptorOnly,
}

impl SourceTeletextCaptionCandidate {
    /// Hearing-impaired subtitle role, independent from forced/default policy.
    #[must_use]
    pub const fn is_sdh(&self) -> bool {
        self.descriptor_type == 5
    }
}

/// Load bounded caption candidates against one exact item/version/file/path.
///
/// All database reads share one read-only snapshot and reuse the source report's
/// current-parent validation. Missing reports, unbound/nonlocal sources and
/// unobserved pages supply no choices. Distinct programs, pages, languages and
/// subtitle roles remain distinct; subpages update the same page choice.
/// This does not read media, run a decoder, persist readiness or change tracks.
///
/// # Errors
/// Returns a static unavailable error for stale/ambiguous requested ownership,
/// corrupt reports/descriptors, excessive metadata or database failures. Private
/// source paths and stored revision values are never included in errors.
pub async fn load_caption_candidates(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    media_file_id: i64,
    original_path: &str,
) -> Result<Vec<SourceTeletextCaptionCandidate>> {
    load_requests(
        pool,
        media_item_id,
        media_version_id,
        media_file_id,
        original_path,
        DiscoveryMode::ObservedHeader,
    )
    .await
}

/// Discover unqualified wire subtitle requests against one exact source file.
///
/// Reuses the candidate loader's read snapshot, current parent/revision checks,
/// bounds, exact codec/index and actual type-6 program descriptor provenance.
/// The stored current-PSI window may contain no page headers; a descriptor page
/// absent from that prefix remains discoverable, never an observed/ready page.
/// Missing current PSI, missing descriptors, unbound and nonlocal observations
/// supply no requests. Every page/language/role/program remains distinct so
/// callers can reject ambiguity before bounded window preparation. This API
/// performs no source I/O, decoding, ranking or database writes.
///
/// # Errors
/// Rejects stale/ambiguous ownership, malformed descriptors and excessive
/// metadata atomically, without returning partial requests or private locators.
pub async fn discover_caption_requests(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    media_file_id: i64,
    original_path: &str,
) -> Result<Vec<SourceTeletextCaptionDescriptorRequest>> {
    load_requests(
        pool,
        media_item_id,
        media_version_id,
        media_file_id,
        original_path,
        DiscoveryMode::DescriptorOnly,
    )
    .await
    .map(|requests| {
        requests
            .into_iter()
            .map(|candidate| SourceTeletextCaptionDescriptorRequest { candidate })
            .collect()
    })
}

async fn load_requests(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    media_file_id: i64,
    original_path: &str,
    mode: DiscoveryMode,
) -> Result<Vec<SourceTeletextCaptionCandidate>> {
    if [media_item_id, media_version_id, media_file_id]
        .iter()
        .any(|id| *id <= 0)
        || original_path.is_empty()
    {
        return Err(unavailable());
    }
    // read-only transaction: exact file ownership and every report share one snapshot.
    let mut tx = pool.begin().await.map_err(|_| unavailable())?;
    let result = load_in_transaction(
        &mut tx,
        media_item_id,
        media_version_id,
        media_file_id,
        original_path,
        mode,
    )
    .await
    .map_err(|_| unavailable());
    tx.rollback().await.map_err(|_| unavailable())?;
    result
}

async fn load_in_transaction(
    tx: &mut Transaction<'_, Db>,
    item: i64,
    version: i64,
    file: i64,
    path: &str,
    mode: DiscoveryMode,
) -> Result<Vec<SourceTeletextCaptionCandidate>> {
    let row = sqlx::query(
        r#"SELECT v."MovieId", v."EpisodeId", v."MusicVideoId",
        v."OtherVideoId", v."SongId", v."ImageId", v."RemoteStreamId"
        FROM "MediaVersion" v JOIN "MediaFile" f ON f."MediaVersionId"=v."Id"
        WHERE v."Id"=? AND f."Id"=? AND f."Path"=? COLLATE BINARY"#,
    )
    .bind(version)
    .bind(file)
    .bind(path)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| unavailable())?
    .ok_or_else(unavailable)?;
    let mut owner = None;
    for field in [
        "MovieId",
        "EpisodeId",
        "MusicVideoId",
        "OtherVideoId",
        "SongId",
        "ImageId",
        "RemoteStreamId",
    ] {
        if let Some(id) = column::<Option<i64>>(&row, field)? {
            if owner.replace(id).is_some() {
                return Err(unavailable());
            }
        }
    }
    if owner != Some(item) {
        return Err(unavailable());
    }
    let rows = sqlx::query(r#"SELECT s."Id", s."Index",
        (SELECT COUNT(*) FROM "MediaStream" other
         WHERE other."MediaVersionId"=s."MediaVersionId" AND other."Index"=s."Index") AS "IndexMatches"
        FROM "MediaStream" s WHERE s."MediaVersionId"=?
        AND s."MediaStreamKind"=3 AND s."Codec"='dvb_teletext'
        ORDER BY s."Id" LIMIT ?"#)
        .bind(version).bind(i64::try_from(MAX_TELETEXT_PROBE_STREAMS + 1).map_err(|_| unavailable())?)
        .fetch_all(&mut **tx).await.map_err(|_| unavailable())?;
    if rows.len() > MAX_TELETEXT_PROBE_STREAMS {
        return Err(unavailable());
    }
    let mut indexes = BTreeSet::new();
    let mut candidates = Vec::new();
    let mut wire_entries = 0_usize;
    for row in rows {
        let index: i32 = column(&row, "Index")?;
        if index < 0 || !indexes.insert(index) || column::<i64>(&row, "IndexMatches")? != 1 {
            return Err(unavailable());
        }
        let Some(observed) = load(tx, column(&row, "Id")?, file).await? else {
            continue;
        };
        append_candidates(&observed, &mut candidates, &mut wire_entries, mode)?;
    }
    Ok(candidates)
}

fn append_candidates(
    observation: &SourceTeletextObservation,
    candidates: &mut Vec<SourceTeletextCaptionCandidate>,
    wire_entries: &mut usize,
    mode: DiscoveryMode,
) -> Result<()> {
    let (
        SourceTeletextBinding::Bound(binding),
        SourceTeletextPid::Reported(pid),
        SourceTeletextObservedPages::HeadersObserved(window),
    ) = (
        &observation.binding,
        observation.pid,
        &observation.observed_pages,
    )
    else {
        return Ok(());
    };
    if binding.provider != MediaSourceKind::Local {
        return Ok(());
    }
    let observed: BTreeSet<_> = window
        .headers
        .iter()
        .map(|header| (header.wire_magazine, header.wire_page))
        .collect();
    let mut programs = BTreeSet::new();
    let mut choices: BTreeMap<(u16, TeletextPageNumber, String, u8), usize> = BTreeMap::new();
    for program in &window.programs {
        if !programs.insert(program.program_number) {
            return Err(unavailable());
        }
        if program.stream_type != 6 {
            continue;
        }
        let entries =
            parse_source_teletext_descriptors(&program.descriptors).map_err(|_| unavailable())?;
        let total = wire_entries
            .checked_add(entries.len())
            .ok_or_else(unavailable)?;
        if total > MAX_CAPTION_CANDIDATES {
            return Err(unavailable());
        }
        *wire_entries = total;
        for entry in entries {
            if !matches!(entry.descriptor_type, 2 | 5)
                || (matches!(mode, DiscoveryMode::ObservedHeader)
                    && !observed.contains(&(entry.wire_magazine, entry.wire_page)))
            {
                continue;
            }
            let Some(page) = entry.display_page() else {
                continue;
            };
            let key = (
                program.program_number,
                page,
                entry.language.as_str().to_owned(),
                entry.descriptor_type,
            );
            if let Some(&index) = choices.get(&key) {
                candidates[index].descriptor_occurrences.push(entry);
                continue;
            }
            if candidates.len() >= MAX_CAPTION_CANDIDATES {
                return Err(unavailable());
            }
            choices.insert(key, candidates.len());
            candidates.push(SourceTeletextCaptionCandidate {
                request: SourceTeletextPageDecodeRequest {
                    binding: binding.clone(),
                    stream_index: observation.stream_index,
                    pid,
                    program_number: program.program_number,
                    page,
                },
                language: entry.language.clone(),
                descriptor_type: entry.descriptor_type,
                psi_fingerprint: window.psi_fingerprint,
                descriptor_occurrences: vec![entry],
            });
        }
    }
    Ok(())
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "source teletext captions",
        "source caption candidates are unavailable",
    )
}

#[cfg(test)]
#[path = "playback_tests.rs"]
mod tests;
