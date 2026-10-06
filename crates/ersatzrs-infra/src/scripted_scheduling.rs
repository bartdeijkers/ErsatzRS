//! Repository-backed Lua sources, called only by the blocking scheduling worker.
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use ersatzrs_core::{
    playout_builder::PlayoutBuilder,
    repositories::{RepositoryError, Result},
    scheduling::{
        BlockSchedulingEngine, ClassicSchedulingEngine, ExternalJsonSchedulingEngine,
        PlayoutSchedulingEngine, SchedulingContentSource, SchedulingError, SchedulingMediaItem,
        SchedulingPlaylistItem, SchedulingRequest, SchedulingResult, ScriptedContentResolver,
        ScriptedSchedulingEngine, ScriptedSourceDependency, ScriptedSourceResolution,
        YamlPlayoutEngine,
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
    dependencies: Mutex<BTreeMap<String, ScriptedSourceDependency>>,
}

impl std::fmt::Debug for SqlSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlSources").finish_non_exhaustive()
    }
}

impl SqlSources {
    fn resolve(
        &self,
        source: &SchedulingContentSource,
        resolution: ScriptedSourceResolution,
    ) -> Result<crate::scripted_dependencies::ResolvedSource> {
        let (resolved, dependency) = self.lookup(crate::scripted_dependencies::resolve(
            &self.repository,
            source,
            resolution,
        ))?;
        let key = crate::scripted_dependencies::source_key(&dependency)?;
        let mut dependencies = self.dependencies.lock().map_err(|_| {
            RepositoryError::unavailable("scripted source", "dependency capture failed")
        })?;
        if dependencies
            .get(&key)
            .is_some_and(|previous| previous != &dependency)
        {
            return Err(RepositoryError::Conflict(
                "Scripted source changed during generation; retry the build".to_owned(),
            ));
        }
        dependencies.insert(key, dependency);
        Ok(resolved)
    }

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
        let crate::scripted_dependencies::ResolvedSource::Media(items) =
            self.resolve(source, ScriptedSourceResolution::Media)?
        else {
            unreachable!("media lookup returns media")
        };
        Ok(items)
    }

    fn playlist_items(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Option<Vec<SchedulingPlaylistItem>>> {
        let crate::scripted_dependencies::ResolvedSource::Playlist(items) =
            self.resolve(source, ScriptedSourceResolution::Playlist)?
        else {
            unreachable!("playlist lookup returns playlist")
        };
        Ok(items)
    }

    fn watermark_id(&self, name: &str) -> Result<Option<EntityId>> {
        self.lookup(self.repository.watermark_id_by_name(name))
    }

    fn graphics_id(&self, path: &str) -> Result<Option<EntityId>> {
        self.lookup(self.repository.graphics_element_id_by_path(path))
    }
}

struct SqlScriptedEngine {
    pool: SqlitePool,
    runtime: tokio::runtime::Handle,
    cancelled: Option<PlayoutBuildCancellation>,
}

impl PlayoutSchedulingEngine for SqlScriptedEngine {
    fn schedule(
        &self,
        request: SchedulingRequest,
    ) -> std::result::Result<SchedulingResult, SchedulingError> {
        let sources = Arc::new(SqlSources {
            repository: SqliteSchedulingContentRepository::new(self.pool.clone()),
            runtime: self.runtime.clone(),
            cancelled: self.cancelled.clone(),
            dependencies: Mutex::default(),
        });
        let mut result =
            ScriptedSchedulingEngine.schedule_with_resolver(request, Some(sources.clone()))?;
        result.scripted_sources =
            std::mem::take(&mut *sources.dependencies.lock().map_err(|_| {
                SchedulingError::Script("Scripted dependency capture failed".to_owned())
            })?)
            .into_values()
            .collect();
        Ok(result)
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
        Box::new(SqlScriptedEngine {
            pool,
            runtime,
            cancelled,
        }),
        Box::new(ExternalJsonSchedulingEngine),
    )
}
