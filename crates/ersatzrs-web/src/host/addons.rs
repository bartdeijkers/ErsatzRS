use async_trait::async_trait;
use ersatzrs_app::{
    AddonOverviewRead, AddonRepositoryEdit, AddonRepositoryRead, AddonRuntime, AddonRuntimeError,
    AddonSettingEdit, AddonSettingRead, AvailableAddonRead, InstalledAddonRead,
};
use ersatzrs_infra::addons::{
    check_addon, disable_addon, enable_addon, install_addon, list_available_addons,
    list_installed_addons, list_trusted_repositories, load_addon_settings, refresh_repository,
    remove_trusted_repository, repositories_are_deployment_managed, save_addon_setting,
    save_trusted_repository, uninstall_addon, AddonRuntimeContext, AddonSecretReferenceKind,
    AddonSettingValue, TrustedAddonRepository,
};
use ersatzrs_infra::db::DbPool;

#[derive(Clone)]
pub struct NativeAddonRuntime {
    pool: DbPool,
    client: reqwest::Client,
}

impl NativeAddonRuntime {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self {
            pool,
            client: addon_repository_client(),
        }
    }
}

pub(crate) fn addon_repository_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[async_trait]
impl AddonRuntime for NativeAddonRuntime {
    async fn overview(&self) -> Result<AddonOverviewRead, AddonRuntimeError> {
        let repositories = list_trusted_repositories(&self.pool)
            .await
            .map_err(runtime_error)?
            .into_iter()
            .map(map_repository)
            .collect();
        let available = list_available_addons(&self.pool)
            .await
            .map_err(runtime_error)?
            .into_iter()
            .map(|value| AvailableAddonRead {
                repository_id: value.repository_id,
                addon: value.addon,
                host_compatible: value.host_compatible,
                package_available: value.package_available,
            })
            .collect();
        let mut installed = Vec::new();
        for value in list_installed_addons(&self.pool)
            .await
            .map_err(runtime_error)?
        {
            let settings = load_addon_settings(&self.pool, &value.addon_id)
                .await
                .map_err(runtime_error)?
                .into_iter()
                .map(|setting| AddonSettingRead {
                    key: setting.key,
                    value: setting.value,
                    reference_kind: setting.reference_kind.map(|kind| match kind {
                        AddonSecretReferenceKind::Environment => "environment".to_owned(),
                        AddonSecretReferenceKind::File => "file".to_owned(),
                    }),
                    reference: setting.reference,
                })
                .collect();
            installed.push(InstalledAddonRead {
                addon_id: value.addon_id,
                repository_id: value.repository_id,
                version: value.version.to_string(),
                enabled: value.enabled,
                manifest: value.manifest,
                settings,
                last_check_code: value.last_check_code,
                last_check_message: value.last_check_message,
            });
        }
        Ok(AddonOverviewRead {
            repositories_are_deployment_managed: repositories_are_deployment_managed(),
            repositories,
            available,
            installed,
        })
    }

    async fn save_repository(
        &self,
        repository: AddonRepositoryEdit,
    ) -> Result<(), AddonRuntimeError> {
        save_trusted_repository(
            &self.pool,
            &TrustedAddonRepository {
                id: repository.id,
                name: repository.name,
                index_url: repository.index_url,
                public_key: repository.public_key,
                is_official: false,
                is_enabled: repository.is_enabled,
                last_sequence: None,
                last_refresh: None,
                last_error: None,
            },
        )
        .await
        .map_err(runtime_error)
    }

    async fn remove_repository(&self, repository_id: String) -> Result<(), AddonRuntimeError> {
        remove_trusted_repository(&self.pool, &repository_id)
            .await
            .map_err(runtime_error)
    }

    async fn refresh_repository(&self, repository_id: String) -> Result<(), AddonRuntimeError> {
        refresh_repository(&self.pool, &self.client, &repository_id)
            .await
            .map(|_| ())
            .map_err(runtime_error)
    }

    async fn install(
        &self,
        repository_id: String,
        addon_id: String,
    ) -> Result<(), AddonRuntimeError> {
        install_addon(&self.pool, &self.client, &repository_id, &addon_id)
            .await
            .map(|_| ())
            .map_err(runtime_error)
    }

    async fn save_setting(&self, setting: AddonSettingEdit) -> Result<(), AddonRuntimeError> {
        let reference_kind = setting
            .reference_kind
            .as_deref()
            .map(|kind| match kind {
                "environment" => Ok(AddonSecretReferenceKind::Environment),
                "file" => Ok(AddonSecretReferenceKind::File),
                _ => Err(AddonRuntimeError::new("unknown secret reference kind")),
            })
            .transpose()?;
        save_addon_setting(
            &self.pool,
            &setting.addon_id,
            &AddonSettingValue {
                key: setting.key,
                value: setting.value,
                reference_kind,
                reference: setting.reference,
            },
        )
        .await
        .map_err(runtime_error)
    }

    async fn check(&self, addon_id: String) -> Result<String, AddonRuntimeError> {
        check_addon(&self.pool, &addon_id, &runtime_context())
            .await
            .map(|result| format!("{}: {}", result.code, result.message))
            .map_err(runtime_error)
    }

    async fn enable(&self, addon_id: String) -> Result<String, AddonRuntimeError> {
        enable_addon(&self.pool, &addon_id, &runtime_context())
            .await
            .map(|result| format!("{}: {}", result.code, result.message))
            .map_err(runtime_error)
    }

    async fn disable(&self, addon_id: String) -> Result<(), AddonRuntimeError> {
        disable_addon(&self.pool, &addon_id)
            .await
            .map_err(runtime_error)
    }

    async fn uninstall(&self, addon_id: String, force: bool) -> Result<(), AddonRuntimeError> {
        uninstall_addon(&self.pool, &addon_id, force)
            .await
            .map_err(runtime_error)
    }
}

fn runtime_context() -> AddonRuntimeContext {
    AddonRuntimeContext {
        ffmpeg_path: Some(ersatzrs_ffmpeg::ffmpeg_path()),
        ffprobe_path: Some(ersatzrs_ffmpeg::ffprobe_path()),
    }
}

fn map_repository(value: TrustedAddonRepository) -> AddonRepositoryRead {
    AddonRepositoryRead {
        id: value.id,
        name: value.name,
        index_url: value.index_url,
        public_key: value.public_key,
        is_official: value.is_official,
        is_enabled: value.is_enabled,
        last_sequence: value.last_sequence,
        last_refresh: value.last_refresh,
        last_error: value.last_error,
    }
}

fn runtime_error(error: impl std::fmt::Display) -> AddonRuntimeError {
    AddonRuntimeError::new(error.to_string())
}
