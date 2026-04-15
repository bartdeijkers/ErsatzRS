use async_trait::async_trait;
use ersatzrs_domain::{EntityId, Library};

use super::Result;

#[async_trait]
pub trait LibraryRepository: Send + Sync {
    async fn get_with_paths(&self, id: EntityId) -> Result<Library>;
    async fn list_for_source(&self, source_id: EntityId) -> Result<Vec<Library>>;
    async fn upsert(&self, library: &Library) -> Result<()>;
}
