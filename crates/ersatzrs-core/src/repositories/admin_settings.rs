use async_trait::async_trait;
use ersatzrs_domain::EntityId;
use serde::{Deserialize, Serialize};

use super::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsOptionKind {
    FfmpegProfile,
    Watermark,
    FallbackFiller,
    CustomResolution,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsSelectOption {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsMutation {
    Set { key: String, value: String },
    Delete { key: String },
}

#[async_trait]
pub trait AdminSettingsRepository: Send + Sync {
    async fn get_optional_string(&self, key: &str) -> Result<Option<String>>;
    async fn set_string(&self, key: &str, value: &str) -> Result<()>;
    async fn delete_key(&self, key: &str) -> Result<()>;
    async fn apply_mutations(&self, mutations: Vec<SettingsMutation>) -> Result<()>;
    async fn list_options(&self, kind: SettingsOptionKind) -> Result<Vec<SettingsSelectOption>>;
    async fn list_playout_ids(&self) -> Result<Vec<EntityId>>;
    async fn create_custom_resolution(&self, width: i32, height: i32) -> Result<()>;
    async fn delete_custom_resolution(&self, id: EntityId) -> Result<()>;
}
