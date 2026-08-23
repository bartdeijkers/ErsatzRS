use ersatzrs_addon_contract::{AddonManifest, CatalogAddon};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonRepositoryRow {
    pub id: String,
    pub name: String,
    pub index_url: String,
    pub public_key: String,
    pub is_official: bool,
    pub is_enabled: bool,
    pub trust_tier: String,
    pub last_sequence: Option<u64>,
    pub last_refresh: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableAddonRow {
    pub repository_id: String,
    pub addon: CatalogAddon,
    pub host_compatible: bool,
    pub package_available: bool,
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonSettingRow {
    pub key: String,
    pub value: Option<String>,
    pub reference_kind: Option<String>,
    pub reference: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledAddonRow {
    pub addon_id: String,
    pub repository_id: Option<String>,
    pub version: String,
    pub enabled: bool,
    pub trust_tier: String,
    pub manifest: AddonManifest,
    pub settings: Vec<AddonSettingRow>,
    pub last_check_code: Option<String>,
    pub last_check_message: Option<String>,
    pub last_check_at: Option<String>,
    pub installed_at: String,
    pub updated_at: String,
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonOverview {
    pub unverified_sources_enabled: bool,
    /// Add-on used for source URLs no installed package claims, if configured.
    pub default_remote_stream_addon: Option<String>,
    pub detail_wait: AddonDetailWaitSettingsForm,
    pub repositories_are_deployment_managed: bool,
    pub repositories: Vec<AddonRepositoryRow>,
    pub available: Vec<AvailableAddonRow>,
    pub installed: Vec<InstalledAddonRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonDetailWaitSettingsForm {
    pub minimum_seconds: i32,
    pub maximum_seconds: i32,
}

impl Default for AddonDetailWaitSettingsForm {
    fn default() -> Self {
        Self {
            minimum_seconds: 2,
            maximum_seconds: 16,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonDetail {
    pub addon_id: String,
    pub installed: Option<InstalledAddonRow>,
    pub offers: Vec<AvailableAddonRow>,
    pub repositories: Vec<AddonRepositoryRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonRepositoryEditor {
    pub id: String,
    pub name: String,
    pub index_url: String,
    pub public_key: String,
    pub is_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonSettingEditor {
    pub addon_id: String,
    pub key: String,
    pub value: Option<String>,
    pub reference_kind: Option<String>,
    pub reference: Option<String>,
}

#[server]
pub async fn load_addon_overview() -> Result<AddonOverview, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let mut overview = state
        .application
        .addon_overview()
        .await
        .map(AddonOverview::from)
        .map_err(ServerFnError::new)?;
    overview.default_remote_stream_addon = state
        .application
        .default_remote_stream_addon()
        .await
        .map_err(ServerFnError::new)?;
    let detail_wait = state
        .application
        .load_addon_detail_wait_settings()
        .await
        .map_err(ServerFnError::new)?;
    overview.detail_wait = AddonDetailWaitSettingsForm {
        minimum_seconds: i32::try_from(detail_wait.minimum_seconds).map_err(ServerFnError::new)?,
        maximum_seconds: i32::try_from(detail_wait.maximum_seconds).map_err(ServerFnError::new)?,
    };
    Ok(overview)
}

/// Persist the global fair-use window used by all deferred item-detail work.
#[server]
pub async fn save_addon_detail_wait_settings(
    settings: AddonDetailWaitSettingsForm,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .save_addon_detail_wait_settings(ersatzrs_app::commands::SaveAddonDetailWaitSettings {
            minimum_seconds: settings.minimum_seconds,
            maximum_seconds: settings.maximum_seconds,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn load_addon_detail(addon_id: String) -> Result<Option<AddonDetail>, ServerFnError> {
    let overview = load_addon_overview().await?;
    let installed = overview
        .installed
        .into_iter()
        .find(|item| item.addon_id == addon_id);
    let offers = overview
        .available
        .into_iter()
        .filter(|item| item.addon.id == addon_id)
        .collect::<Vec<_>>();
    if installed.is_none() && offers.is_empty() {
        return Ok(None);
    }
    Ok(Some(AddonDetail {
        addon_id,
        installed,
        offers,
        repositories: overview.repositories,
    }))
}

#[server]
pub async fn set_unverified_addon_sources(enabled: bool) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .set_unverified_addon_sources(enabled)
        .await
        .map_err(ServerFnError::new)
}

/// Persist or clear the add-on used for unclaimed remote source URLs.
#[server]
pub async fn set_default_remote_stream_addon(addon_id: String) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let addon_id = addon_id.trim().to_owned();
    expect_context::<AppState>()
        .application
        .set_default_remote_stream_addon((!addon_id.is_empty()).then_some(addon_id))
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn save_addon_repository(repository: AddonRepositoryEditor) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .save_addon_repository(ersatzrs_app::AddonRepositoryEdit {
            id: repository.id,
            name: repository.name,
            index_url: repository.index_url,
            public_key: repository.public_key,
            is_enabled: repository.is_enabled,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn remove_addon_repository(repository_id: String) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .remove_addon_repository(repository_id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn refresh_addon_repository(repository_id: String) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .refresh_addon_repository(repository_id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn install_addon_package(
    repository_id: String,
    addon_id: String,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .install_addon(repository_id, addon_id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn save_addon_configuration(setting: AddonSettingEditor) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .save_addon_setting(ersatzrs_app::AddonSettingEdit {
            addon_id: setting.addon_id,
            key: setting.key,
            value: setting.value,
            reference_kind: setting.reference_kind,
            reference: setting.reference,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn check_addon_package(addon_id: String) -> Result<String, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .check_addon(addon_id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn enable_addon_package(addon_id: String) -> Result<String, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .enable_addon(addon_id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn disable_addon_package(addon_id: String) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .disable_addon(addon_id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn uninstall_addon_package(addon_id: String, force: bool) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .uninstall_addon(addon_id, force)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_app::AddonOverviewRead> for AddonOverview {
    fn from(value: ersatzrs_app::AddonOverviewRead) -> Self {
        Self {
            unverified_sources_enabled: value.unverified_sources_enabled,
            // Loaded alongside the overview rather than through the read model,
            // which the add-on lifecycle owns.
            default_remote_stream_addon: None,
            detail_wait: AddonDetailWaitSettingsForm::default(),
            repositories_are_deployment_managed: value.repositories_are_deployment_managed,
            repositories: value
                .repositories
                .into_iter()
                .map(|item| AddonRepositoryRow {
                    id: item.id,
                    name: item.name,
                    index_url: item.index_url,
                    public_key: item.public_key,
                    is_official: item.is_official,
                    is_enabled: item.is_enabled,
                    trust_tier: item.trust_tier,
                    last_sequence: item.last_sequence,
                    last_refresh: item.last_refresh,
                    last_error: item.last_error,
                })
                .collect(),
            available: value
                .available
                .into_iter()
                .map(|item| AvailableAddonRow {
                    repository_id: item.repository_id,
                    addon: item.addon,
                    host_compatible: item.host_compatible,
                    package_available: item.package_available,
                    icon_url: item.icon_url,
                })
                .collect(),
            installed: value
                .installed
                .into_iter()
                .map(|item| InstalledAddonRow {
                    addon_id: item.addon_id,
                    repository_id: item.repository_id,
                    version: item.version,
                    enabled: item.enabled,
                    trust_tier: item.trust_tier,
                    manifest: item.manifest,
                    settings: item
                        .settings
                        .into_iter()
                        .map(|setting| AddonSettingRow {
                            key: setting.key,
                            value: setting.value,
                            reference_kind: setting.reference_kind,
                            reference: setting.reference,
                        })
                        .collect(),
                    last_check_code: item.last_check_code,
                    last_check_message: item.last_check_message,
                    last_check_at: item.last_check_at,
                    installed_at: item.installed_at,
                    updated_at: item.updated_at,
                    icon_url: item.icon_url,
                })
                .collect(),
        }
    }
}
