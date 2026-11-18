//! Exact extraction input and ADR0137 logical-to-physical stream binding.
use super::*;
use crate::playback::source_indexes;

pub(super) struct ExtractionInput {
    pub(super) path: String,
    original: String,
    pub(super) item: i64,
    version: i64,
    file: i64,
    indexes: Option<source_indexes::StreamIndexes>,
}

pub(super) async fn resolve_input_path(
    pool: &Pool<Db>,
    item: i64,
    replacements: &[PathReplacement],
) -> Result<Option<ExtractionInput>> {
    let row = sqlx::query(
        r#"SELECT mf."Path",mf."Id" AS "File",mv."Id" AS "Version"
        FROM "MediaVersion" mv JOIN "MediaFile" mf ON mf."MediaVersionId"=mv."Id"
        WHERE COALESCE(mv."EpisodeId",mv."MovieId",mv."MusicVideoId",mv."OtherVideoId")=?
        AND mf."Path" IS NOT NULL ORDER BY mv."Id",mf."Id" LIMIT 1"#,
    )
    .bind(item)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else { return Ok(None) };
    let original: String = row.try_get("Path").repository()?;
    let version = row.try_get("Version").repository()?;
    let indexes = source_indexes::load(pool, version).await?;
    let own = match &indexes {
        Some(indexes) => Some(
            crate::media_server::load_path_replacements_from(
                pool,
                "JellyfinPathReplacement",
                "JellyfinPath",
                "JellyfinMediaSourceId",
                indexes.source_id(),
            )
            .await
            .repository()?,
        ),
        None => None,
    };
    let path = replace_path(
        own.as_deref().unwrap_or(replacements),
        &original,
        crate::media_server::HOST_PLATFORM,
    );
    Ok(Some(ExtractionInput {
        path,
        original,
        item,
        version,
        file: row.try_get("File").repository()?,
        indexes,
    }))
}
impl ExtractionInput {
    pub(super) async fn map_candidates(
        &self,
        pool: &Pool<Db>,
        candidates: &mut [SubtitleToExtract],
    ) -> Result<()> {
        if self.indexes.is_some() {
            let allowed =
                crate::playback::load_subtitles_for_media_version(pool, self.item, self.version)
                    .await?;
            for sub in candidates {
                if !allowed.iter().any(|row| row.id == sub.id) {
                    return Err(unavailable());
                }
                sub.physical_index = i32::try_from(source_indexes::physical(
                    self.indexes.as_ref(),
                    usize::try_from(sub.stream_index).map_err(|_| unavailable())?,
                )?)
                .map_err(|_| unavailable())?;
            }
        }
        source_indexes::ensure_current(pool, self.version, self.indexes.as_ref()).await
    }
    pub(super) async fn validate(&self, tx: &mut sqlx::Transaction<'_, Db>) -> Result<()> {
        let current:bool=sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "MediaFile" f JOIN "MediaVersion" v ON v."Id"=f."MediaVersionId"
            WHERE f."Id"=? AND v."Id"=? AND f."Path"=? AND COALESCE(v."EpisodeId",v."MovieId",v."MusicVideoId",v."OtherVideoId")=?)"#)
            .bind(self.file).bind(self.version).bind(&self.original).bind(self.item).fetch_one(&mut **tx).await.repository()?;
        if !current || source_indexes::load(&mut **tx, self.version).await? != self.indexes {
            return Err(unavailable());
        }
        if let Some(indexes) = &self.indexes {
            let mappings = crate::media_server::load_path_replacements_from(
                &mut **tx,
                "JellyfinPathReplacement",
                "JellyfinPath",
                "JellyfinMediaSourceId",
                indexes.source_id(),
            )
            .await
            .repository()?;
            if replace_path(
                &mappings,
                &self.original,
                crate::media_server::HOST_PLATFORM,
            ) != self.path
            {
                return Err(unavailable());
            }
        }
        Ok(())
    }
}
pub(super) fn unavailable() -> ersatzrs_core::repositories::RepositoryError {
    ersatzrs_core::repositories::RepositoryError::Conflict(
        "Subtitle extraction source or stream mapping changed; rescan before retrying".into(),
    )
}
