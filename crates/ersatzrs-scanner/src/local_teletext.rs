//! Caller-owned local scan projection and optional, claim-fenced report writes.
//!
//! The leaf writer owns its transaction and must roll back every error. Source
//! acquisition is report-only: it neither selects captions nor observes pages.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ersatzrs_core::repositories::{ManagedLibraryOperationClaim, RepositoryError};
use ersatzrs_domain::{
    MediaSourceKind, SourceTeletextBinding, SourceTeletextBindingIdentity,
    SourceTeletextDescriptor, SourceTeletextDescriptors, SourceTeletextObservation,
    SourceTeletextPid, SourceTeletextProgram, SourceTeletextPrograms, SourceTeletextProvenance,
};
use ersatzrs_infra::db::Db;
use ersatzrs_infra::ffprobe::teletext::TeletextProjectionWarning;
use ersatzrs_infra::ffprobe::teletext_probe::{
    ProbedTeletextStream, TeletextProbeDescriptors, TeletextProbePid, TeletextProbePrograms,
    TeletextProbeSnapshot,
};
use ersatzrs_infra::ffprobe::{FfprobeExtractor, FfprobeMediaVersion, LocalTeletextAcquisition};
use ersatzrs_infra::repositories::claim_is_current;
use ersatzrs_infra::teletext_source::{self, SourceTeletextParents};
use sqlx::Transaction;

use crate::Result;

#[derive(Clone)]
pub(crate) struct LocalScanProbe {
    pub media: FfprobeMediaVersion,
    pub teletext: LocalTeletextWrite,
}

impl From<FfprobeMediaVersion> for LocalScanProbe {
    fn from(media: FfprobeMediaVersion) -> Self {
        Self {
            media,
            teletext: LocalTeletextWrite {
                claim: None,
                acquisition: None,
                source_path: None,
                report_written: false,
            },
        }
    }
}

/// Retains the actual operation claim and original source handle until commit.
#[derive(Clone, Default)]
pub(crate) struct LocalTeletextWrite {
    claim: Option<ManagedLibraryOperationClaim>,
    acquisition: Option<Arc<LocalTeletextAcquisition>>,
    source_path: Option<PathBuf>,
    report_written: bool,
}

pub(crate) async fn acquire(
    extractor: &FfprobeExtractor,
    path: &Path,
    claim: Option<&ManagedLibraryOperationClaim>,
) -> Result<LocalScanProbe> {
    let acquired = extractor.extract_local_with_teletext(path).await?;
    Ok(LocalScanProbe {
        media: acquired.media,
        teletext: LocalTeletextWrite {
            claim: claim.cloned(),
            acquisition: acquired.teletext,
            source_path: Some(path.to_owned()),
            report_written: false,
        },
    })
}

impl LocalTeletextWrite {
    /// Call before the first ordinary mutation and immediately before commit.
    /// After any report write a changed source requires whole-leaf rollback.
    pub(crate) async fn check_claim(
        &self,
        tx: &mut Transaction<'_, Db>,
        library_path_id: i64,
    ) -> Result<()> {
        let Some(claim) = &self.claim else {
            return Ok(());
        };
        // Bind the DTO's library to both the actual leaf and actual lease. The
        // existing gate below owns token/owner/expiry evaluation, including the
        // parent lease used by publication child scans.
        let library_matches: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "LibraryPath" p
               JOIN "ManagedLibraryLease" l ON l."LibraryId"=p."LibraryId"
               WHERE p."Id"=? AND p."LibraryId"=? AND l."OperationId"=?
                 AND l."OwnerToken"=? AND l."LeaseOwner"=?)"#,
        )
        .bind(library_path_id)
        .bind(claim.operation.library_id)
        .bind(
            claim
                .operation
                .parent_operation_id
                .unwrap_or(claim.operation.id),
        )
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .fetch_one(&mut **tx)
        .await?;
        if !library_matches || !claim_is_current(tx, claim).await? {
            return Err(conflict("local scan claim is no longer current"));
        }
        if self.report_written
            && !self
                .source_path
                .as_deref()
                .zip(self.acquisition.as_deref())
                .is_some_and(|(path, acquisition)| acquisition.is_current(path))
        {
            return Err(conflict("local source changed after teletext report write"));
        }
        Ok(())
    }

    /// Persist only after the ordinary writer has established fresh stream/file
    /// rows. No transaction, retry, commit or source probe is started here.
    pub(crate) async fn persist(
        &mut self,
        tx: &mut Transaction<'_, Db>,
        library_path_id: i64,
        version_id: i64,
        path: &Path,
    ) -> Result<()> {
        self.check_claim(tx, library_path_id).await?;
        let (Some(claim), Some(acquisition)) = (&self.claim, &self.acquisition) else {
            return Ok(());
        };
        if self.source_path.as_deref() != Some(path) {
            return Err(conflict("local teletext source association changed"));
        }
        if !acquisition.is_current(path) {
            return Ok(()); // No report writes yet; ordinary metadata may commit.
        }
        let TeletextProbeSnapshot::Available(streams) = acquisition.snapshot() else {
            return Ok(());
        };
        let context = parents_context(tx, library_path_id, version_id, path).await?;
        for stream in streams {
            let parents =
                fresh_parents(tx, version_id, context.file_id, stream.stream_index).await?;
            let observation = observation(context.source_id, parents, acquisition, stream)?;
            if !acquisition.is_current(path) {
                if self.report_written {
                    return Err(conflict(
                        "local source changed during teletext report write",
                    ));
                }
                return Ok(());
            }
            if !teletext_source::replace(tx, claim, parents, &observation).await? {
                // Ordinary rows have already been changed by our caller. Stale
                // admission therefore requires rollback, never a successful skip.
                return Err(conflict(
                    "local scan claim changed during teletext report write",
                ));
            }
            self.report_written = true;
        }
        self.check_claim(tx, library_path_id).await
    }
}

struct ParentsContext {
    source_id: i64,
    file_id: i64,
}

async fn parents_context(
    tx: &mut Transaction<'_, Db>,
    library_path_id: i64,
    version_id: i64,
    path: &Path,
) -> Result<ParentsContext> {
    let sources: Vec<i64> = sqlx::query_scalar(
        r#"SELECT l."MediaSourceId" FROM "LibraryPath" p
           JOIN "Library" l ON l."Id"=p."LibraryId"
           JOIN "LocalLibrary" ll ON ll."Id"=l."Id"
           JOIN "LocalMediaSource" ls ON ls."Id"=l."MediaSourceId"
           WHERE p."Id"=? LIMIT 2"#,
    )
    .bind(library_path_id)
    .fetch_all(&mut **tx)
    .await?;
    let files: Vec<i64> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "MediaFile" WHERE "MediaVersionId"=? AND "Path"=? LIMIT 2"#,
    )
    .bind(version_id)
    .bind(path.display().to_string())
    .fetch_all(&mut **tx)
    .await?;
    let ([source_id], [file_id]) = (sources.as_slice(), files.as_slice()) else {
        return Err(conflict(
            "local teletext requires exact current source and file parents",
        ));
    };
    Ok(ParentsContext {
        source_id: *source_id,
        file_id: *file_id,
    })
}

async fn fresh_parents(
    tx: &mut Transaction<'_, Db>,
    version_id: i64,
    file_id: i64,
    stream_index: i32,
) -> Result<SourceTeletextParents> {
    let streams: Vec<i64> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "MediaStream" WHERE "MediaVersionId"=? AND "Index"=?
           AND "MediaStreamKind"=3 AND "Codec"='dvb_teletext' LIMIT 2"#,
    )
    .bind(version_id)
    .bind(stream_index)
    .fetch_all(&mut **tx)
    .await?;
    let [stream_id] = streams.as_slice() else {
        return Err(conflict(
            "local teletext requires one exact current subtitle stream",
        ));
    };
    Ok(SourceTeletextParents {
        media_version_id: version_id,
        media_stream_id: *stream_id,
        media_file_id: file_id,
    })
}

fn observation(
    source_id: i64,
    parents: SourceTeletextParents,
    acquisition: &LocalTeletextAcquisition,
    stream: &ProbedTeletextStream,
) -> Result<SourceTeletextObservation> {
    let binding = SourceTeletextBindingIdentity::new(
        source_id,
        MediaSourceKind::Local,
        parents.media_version_id,
        parents.media_file_id,
        parents.media_stream_id,
        acquisition.revision().clone(),
    )
    .map_err(|_| conflict("invalid local teletext parent binding"))?;
    let pid = match stream.pid {
        TeletextProbePid::Unavailable => SourceTeletextPid::Unavailable,
        TeletextProbePid::Unusable => SourceTeletextPid::Unusable,
        TeletextProbePid::Available(pid) => SourceTeletextPid::Reported(pid),
    };
    SourceTeletextObservation::new(
        SourceTeletextBinding::Bound(binding),
        stream.stream_index,
        SourceTeletextProvenance::Ffprobe9PackedDescriptorProjection,
        pid,
        programs(&stream.programs)?,
        descriptors(&stream.descriptors)?,
    )
    .map_err(|_| conflict("invalid local teletext projection"))
}

fn programs(programs: &TeletextProbePrograms) -> Result<SourceTeletextPrograms> {
    Ok(match programs {
        TeletextProbePrograms::Unavailable(_) => SourceTeletextPrograms::Unavailable,
        TeletextProbePrograms::Unusable(_) => SourceTeletextPrograms::Unusable,
        TeletextProbePrograms::Available(programs) => {
            if programs.len() > teletext_source::MAX_SOURCE_TELETEXT_PROGRAMS {
                return Err(conflict(
                    "local teletext program projection exceeds its bound",
                ));
            }
            SourceTeletextPrograms::Reported(
                programs
                    .iter()
                    .map(|program| SourceTeletextProgram {
                        program_id: program.program_id,
                        program_number: program.program_number,
                        pmt_pid: program.pmt_pid,
                        pcr_pid: program.pcr_pid,
                    })
                    .collect(),
            )
        }
    })
}

fn descriptors(descriptors: &TeletextProbeDescriptors) -> Result<SourceTeletextDescriptors> {
    Ok(match descriptors {
        TeletextProbeDescriptors::Unavailable(_) => SourceTeletextDescriptors::Unavailable,
        TeletextProbeDescriptors::Unusable(_) => SourceTeletextDescriptors::Unusable,
        TeletextProbeDescriptors::Available(descriptors) => {
            if descriptors.entries.len() > teletext_source::MAX_SOURCE_TELETEXT_DESCRIPTORS {
                return Err(conflict(
                    "local teletext descriptor projection exceeds its bound",
                ));
            }
            let entries = descriptors
                .entries
                .iter()
                .map(|entry| {
                    SourceTeletextDescriptor::new(
                        u32::try_from(entry.projection_ordinal)
                            .map_err(|_| conflict("local teletext ordinal exceeds its bound"))?,
                        entry.language.clone(),
                        entry.descriptor_type,
                        entry.wire_magazine,
                        entry.wire_page,
                    )
                    .map_err(|_| conflict("invalid local teletext descriptor"))
                })
                .collect::<Result<Vec<_>>>()?;
            let trailing_unpaired_pairs = descriptors
                .warning
                .map(|warning| match warning {
                    TeletextProjectionWarning::TrailingUnpairedPairs { count } => {
                        u32::try_from(count).map_err(|_| {
                            conflict("local teletext trailing pairs exceed their bound")
                        })
                    }
                })
                .transpose()?;
            SourceTeletextDescriptors::Reported {
                entries,
                trailing_unpaired_pairs,
            }
        }
    })
}

fn conflict(message: &'static str) -> crate::ScannerError {
    RepositoryError::Conflict(message.to_owned()).into()
}

#[cfg(test)]
mod tests;
