use async_trait::async_trait;

use super::Result;

#[async_trait]
pub trait ConfigRepository: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<String>>;
    async fn set(&self, key: &str, value: &str) -> Result<()>;
}
