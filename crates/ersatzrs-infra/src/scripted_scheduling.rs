//! Repository-backed Lua sources, called only by the blocking scheduling worker.
use std::{future::Future, sync::Arc, time::Duration};

use ersatzrs_core::{
    playout_builder::PlayoutBuilder,
    repositories::{RepositoryError, Result},
    scheduling::{
        BlockSchedulingEngine, ClassicSchedulingEngine, ExternalJsonSchedulingEngine,
        PlayoutSchedulingEngine, SchedulingContentSource, SchedulingError, SchedulingMediaItem,
        SchedulingPlaylistItem, SchedulingRequest, SchedulingResult, ScriptedContentResolver,
        ScriptedSchedulingEngine, YamlPlayoutEngine,
    },
};
use ersatzrs_domain::EntityId;
use sqlx::SqlitePool;

use crate::{
    maintenance::PlayoutBuildCancellation, repositories::SqliteSchedulingContentRepository,
};

struct SqlSources {
    repository: SqliteSchedulingContentRepository,
    runtime: tokio::runtime::Handle,
    cancelled: Option<PlayoutBuildCancellation>,
}

impl std::fmt::Debug for SqlSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlSources").finish_non_exhaustive()
    }
}

impl SqlSources {
    fn lookup<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        let cancelled = || self.cancelled.as_ref().is_some_and(|check| check());
        let unavailable =
            || RepositoryError::unavailable("scripted source", "lookup failed or cancelled");
        if cancelled() {
            return Err(unavailable());
        }
        let result = self
            .runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(30), future).await })
            .map_err(|_| unavailable())?
            .map_err(|_| unavailable())?;
        if cancelled() {
            return Err(unavailable());
        }
        Ok(result)
    }
}

impl ScriptedContentResolver for SqlSources {
    fn media_items(&self, source: &SchedulingContentSource) -> Result<Vec<SchedulingMediaItem>> {
        self.lookup(self.repository.media_items_for_source(source))
    }

    fn playlist_items(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Option<Vec<SchedulingPlaylistItem>>> {
        self.lookup(self.repository.playlist_items_for_source(source))
    }

    fn watermark_id(&self, name: &str) -> Result<Option<EntityId>> {
        self.lookup(self.repository.watermark_id_by_name(name))
    }

    fn graphics_id(&self, path: &str) -> Result<Option<EntityId>> {
        self.lookup(self.repository.graphics_element_id_by_path(path))
    }
}

struct SqlScriptedEngine(Arc<SqlSources>);

impl PlayoutSchedulingEngine for SqlScriptedEngine {
    fn schedule(
        &self,
        request: SchedulingRequest,
    ) -> std::result::Result<SchedulingResult, SchedulingError> {
        ScriptedSchedulingEngine.schedule_with_resolver(request, Some(self.0.clone()))
    }
}

/// Build with the normal engines and repository-backed scripted sources.
/// Invoke the returned builder on a blocking worker, outside the async executor.
#[must_use]
pub fn playout_builder(
    pool: SqlitePool,
    runtime: tokio::runtime::Handle,
    cancelled: Option<PlayoutBuildCancellation>,
) -> PlayoutBuilder {
    PlayoutBuilder::new(
        Box::new(ClassicSchedulingEngine),
        Box::new(BlockSchedulingEngine),
        Box::new(YamlPlayoutEngine),
        Box::new(SqlScriptedEngine(Arc::new(SqlSources {
            repository: SqliteSchedulingContentRepository::new(pool),
            runtime,
            cancelled,
        }))),
        Box::new(ExternalJsonSchedulingEngine),
    )
}
