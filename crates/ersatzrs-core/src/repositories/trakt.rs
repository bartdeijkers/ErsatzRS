use async_trait::async_trait;
use ersatzrs_domain::EntityId;
use serde::{Deserialize, Serialize};

use crate::trakt::TraktListLocator;

use super::Result;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraktListAdminRow {
    pub id: EntityId,
    pub trakt_id: i32,
    pub user: String,
    pub list: String,
    pub name: String,
    pub item_count: i32,
    pub match_count: i32,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraktListAdminForm {
    pub id: EntityId,
    pub user: String,
    pub list: String,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
}

#[async_trait]
pub trait TraktRepository: Send + Sync {
    async fn list_admin(&self, page: u32, page_size: u32) -> Result<(Vec<TraktListAdminRow>, u64)>;
    async fn get_admin_form(&self, id: EntityId) -> Result<Option<TraktListAdminForm>>;
    async fn add_or_refresh(&self, locator: &TraktListLocator) -> Result<EntityId>;
    async fn match_items(&self, id: EntityId) -> Result<()>;
    async fn update_settings(
        &self,
        id: EntityId,
        auto_refresh: bool,
        generate_playlist: bool,
    ) -> Result<()>;
    async fn delete(&self, id: EntityId) -> Result<()>;
    async fn get_page_size(&self) -> Result<u32>;
    async fn set_page_size(&self, page_size: u32) -> Result<()>;
}
