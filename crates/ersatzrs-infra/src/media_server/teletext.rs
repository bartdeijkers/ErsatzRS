//! Report-only enrichment of an explicitly mapped Jellyfin source.
//!
//! Provider indexes are declarations, not native identities. Only an exact
//! selected path and complete embedded track match can bind a held acquisition.

use std::path::PathBuf;
use std::sync::Arc;

use ersatzrs_core::path_replacement::replace_path;
use ersatzrs_core::repositories::{
    ManagedLibraryOperationClaim, ProviderMediaSourceDeclaration, RepositoryError, Result,
};
use ersatzrs_domain::{MediaSourceKind, MediaStream, MediaStreamKind};
use sqlx::Transaction;

use crate::db::{Db, DbPool};
use crate::ffprobe::teletext_probe::TeletextProbeSnapshot;
use crate::ffprobe::{FfprobeError, FfprobeExtractor, LocalTeletextAcquisition};
use crate::repositories::{claim_is_current, SqlxResultExt as _};
use crate::teletext_source::{self, SourceTeletextParents};

/// Lives through the ordinary leaf transaction and its final source/lease check.
pub(crate) struct MappedTeletextWrite {
    claim: Option<ManagedLibraryOperationClaim>,
    source_id: i64,
    provider_path: String,
    local_path: Option<PathBuf>,
    acquisition: Option<Arc<LocalTeletextAcquisition>>,
    report_written: bool,
}

impl MappedTeletextWrite {
    pub(crate) async fn acquire(
        pool: &DbPool,
        claim: Option<&ManagedLibraryOperationClaim>,
        source_id: i64,
        library_id: i64,
        item_path: &str,
        declaration: Option<&ProviderMediaSourceDeclaration>,
        streams: &[MediaStream],
    ) -> Result<Self> {
        let mut write = Self {
            claim: claim.cloned(),
            source_id,
            provider_path: item_path.to_owned(),
            local_path: None,
            acquisition: None,
            report_written: false,
        };
        if claim.is_some() {
            // This read-only transaction checks admission and closes before any source I/O.
            let mut tx = pool.begin().await.repository()?;
            write.check_claim(&mut tx, library_id).await?;
            tx.rollback().await.repository()?;
        }
        let Some(declaration) = declaration.filter(|declaration| {
            claim.is_some()
                && !declaration.source_id.trim().is_empty()
                && declaration.path == item_path
                && declaration_matches(declaration, streams)
                && streams.iter().any(is_teletext)
        }) else {
            return Ok(write);
        };
        let resolved = resolve(pool, source_id, &declaration.path).await?;
        let path = PathBuf::from(resolved);
        // A physical regular file is qualified by the owned native acquisition,
        // never by provider JSON, an availability flag or a mapped path alone.
        let extractor = FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path());
        let probe = extractor
            .extract_mapped_teletext_report(&path)
            .await
            .map_err(|error| probe_error(&error))?;
        if let Some(probe) =
            probe.filter(|probe| native_tracks_match(streams, &probe.media.streams))
        {
            write.local_path = Some(path);
            write.acquisition = probe.teletext;
        }
        Ok(write)
    }

    pub(crate) async fn check_claim(
        &self,
        tx: &mut Transaction<'_, Db>,
        library_id: i64,
    ) -> Result<()> {
        let Some(claim) = &self.claim else {
            return Ok(());
        };
        let matches: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "Library" l
               JOIN "JellyfinLibrary" jl ON jl."Id"=l."Id"
               JOIN "JellyfinMediaSource" s ON s."Id"=l."MediaSourceId"
               JOIN "ManagedLibraryLease" lease ON lease."LibraryId"=l."Id"
               WHERE l."Id"=? AND l."Id"=? AND l."MediaSourceId"=?
                 AND lease."OperationId"=? AND lease."OwnerToken"=? AND lease."LeaseOwner"=?)"#,
        )
        .bind(library_id)
        .bind(claim.operation.library_id)
        .bind(self.source_id)
        .bind(
            claim
                .operation
                .parent_operation_id
                .unwrap_or(claim.operation.id),
        )
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .fetch_one(&mut **tx)
        .await
        .repository()?;
        if !matches || !claim_is_current(tx, claim).await? {
            return Err(conflict("mapped source scan claim is no longer current"));
        }
        if self.report_written && !self.source_current(tx).await? {
            return Err(conflict(
                "mapped source changed after teletext report write",
            ));
        }
        // The bounded source worker may outlive the lease checked above.
        if !claim_is_current(tx, claim).await? {
            return Err(conflict(
                "mapped source scan claim expired during source check",
            ));
        }
        Ok(())
    }

    async fn source_current(&self, tx: &mut Transaction<'_, Db>) -> Result<bool> {
        let (Some(path), Some(acquisition)) = (&self.local_path, &self.acquisition) else {
            return Ok(false);
        };
        if resolve(&mut **tx, self.source_id, &self.provider_path).await? != path.to_string_lossy()
        {
            return Ok(false);
        }
        match acquisition.is_current_owned(path).await {
            Ok(current) => Ok(current),
            Err(
                error @ (FfprobeError::Cancelled { .. } | FfprobeError::CleanupUnconfirmed { .. }),
            ) => Err(probe_error(&error)),
            Err(_) => Ok(false),
        }
    }

    pub(crate) async fn persist(
        &mut self,
        tx: &mut Transaction<'_, Db>,
        library_id: i64,
        version_id: i64,
    ) -> Result<()> {
        self.check_claim(tx, library_id).await?;
        let (Some(claim), Some(acquisition)) = (&self.claim, &self.acquisition) else {
            return Ok(());
        };
        if !self.source_current(tx).await? {
            return Ok(()); // Before any report write, ordinary metadata may commit.
        }
        let TeletextProbeSnapshot::Available(streams) = acquisition.snapshot() else {
            return Ok(());
        };
        for stream in streams {
            let rows: Vec<(i64, i64)> = sqlx::query_as(
                r#"SELECT s."Id", f."Id" FROM "MediaStream" s
                   JOIN "MediaFile" f ON f."MediaVersionId"=s."MediaVersionId"
                   WHERE s."MediaVersionId"=? AND s."Index"=?
                     AND s."MediaStreamKind"=3 AND s."Codec"='dvb_teletext'
                     AND f."Path"=? LIMIT 2"#,
            )
            .bind(version_id)
            .bind(stream.stream_index)
            .bind(&self.provider_path)
            .fetch_all(&mut **tx)
            .await
            .repository()?;
            let [(stream_id, file_id)] = rows.as_slice() else {
                return Err(conflict(
                    "mapped teletext requires exact current stream and file",
                ));
            };
            let parents = SourceTeletextParents {
                media_version_id: version_id,
                media_stream_id: *stream_id,
                media_file_id: *file_id,
            };
            let observation = teletext_source::acquired_observation(
                self.source_id,
                MediaSourceKind::Jellyfin,
                parents,
                acquisition,
                stream,
            )?;
            if !teletext_source::replace(tx, claim, parents, &observation).await? {
                return Err(conflict(
                    "mapped source scan claim changed during report write",
                ));
            }
            self.report_written = true;
        }
        self.check_claim(tx, library_id).await
    }
}

fn declaration_matches(
    declaration: &ProviderMediaSourceDeclaration,
    streams: &[MediaStream],
) -> bool {
    let mut indexes = streams
        .iter()
        .filter(|stream| stream.kind != MediaStreamKind::ExternalSubtitle)
        .map(|stream| stream.index)
        .collect::<Vec<_>>();
    let mut declared = declaration.stream_indexes.clone();
    indexes.sort_unstable();
    declared.sort_unstable();
    !indexes.is_empty()
        && indexes == declared
        && indexes[0] >= 0
        && indexes.windows(2).all(|pair| pair[0] != pair[1])
}

fn native_tracks_match(provider: &[MediaStream], native: &[MediaStream]) -> bool {
    let mut declared = provider
        .iter()
        .filter(|stream| stream.kind != MediaStreamKind::ExternalSubtitle)
        .collect::<Vec<_>>();
    let mut observed = native.iter().collect::<Vec<_>>();
    declared.sort_unstable_by_key(|stream| stream.index);
    observed.sort_unstable_by_key(|stream| stream.index);
    declared.len() == observed.len()
        && declared
            .iter()
            .zip(observed)
            .all(|(a, b)| a.index == b.index && a.kind == b.kind && a.codec == b.codec)
}

fn is_teletext(stream: &MediaStream) -> bool {
    stream.kind != MediaStreamKind::ExternalSubtitle
        && stream.kind == MediaStreamKind::Subtitle
        && stream.codec.as_deref() == Some("dvb_teletext")
}

async fn resolve<'e, E>(executor: E, source_id: i64, path: &str) -> Result<String>
where
    E: sqlx::Executor<'e, Database = Db>,
{
    let replacements = super::load_path_replacements_from(
        executor,
        "JellyfinPathReplacement",
        "JellyfinPath",
        "JellyfinMediaSourceId",
        source_id,
    )
    .await
    .repository()?;
    Ok(replace_path(&replacements, path, super::HOST_PLATFORM))
}

fn probe_error(error: &FfprobeError) -> RepositoryError {
    match error {
        FfprobeError::CleanupUnconfirmed { .. } => {
            conflict("mapped teletext probe cleanup is unconfirmed")
        }
        _ => conflict("mapped teletext probe was cancelled"),
    }
}

fn conflict(message: &'static str) -> RepositoryError {
    RepositoryError::Conflict(message.to_owned())
}

#[cfg(test)]
mod tests;
