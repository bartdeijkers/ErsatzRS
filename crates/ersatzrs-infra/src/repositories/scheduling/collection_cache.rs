//! ErsatzTV fafd9b43 `CollectionItemsCache`: successful immutable material per build.
use super::{BTreeMap, EntityId, Result, SchedulingMediaItem, SqliteSchedulingContentRepository};
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

type SmartItems = Arc<OnceCell<Vec<SchedulingMediaItem>>>;

#[derive(Default)]
pub(super) struct CollectionItemsCache(Mutex<BTreeMap<EntityId, SmartItems>>);

impl SqliteSchedulingContentRepository {
    pub(super) fn for_build(&self) -> Self {
        Self(
            self.0.clone(),
            self.1.clone(),
            Some(Arc::new(CollectionItemsCache::default())),
        )
    }

    pub(super) async fn cached_smart_items(
        &self,
        id: EntityId,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let Some(cache) = &self.2 else {
            return self.load_smart_collection_items(id).await;
        };
        let cell = cache.0.lock().await.entry(id).or_default().clone();
        // Only the immutable source material is shared. Callers apply overrides
        // and build independent enumerators to their own copies. Errors are not cached.
        cell.get_or_try_init(|| self.load_smart_collection_items(id))
            .await
            .cloned()
    }
}
